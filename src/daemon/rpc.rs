//! The socket listener: hooks and status lines in, views out.
//!
//! One task per connection. A hook connection sends one line and waits for
//! one verdict; a subscriber sends one line and then reads forever. Neither
//! may hold up the other, and nothing here may hold up an agent: the inbox is
//! offered to with `try_send`, and a full inbox drops the input rather than
//! making a hook wait.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::SystemTime;

use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixListener;
use tokio::sync::{mpsc, watch};

use crate::daemon::now;
use crate::ingest::frame::{Request, decode_request, error_reply, ok_reply};
use crate::ingest::{Ctx, Input, claude};
use crate::model::ProcKey;
use crate::proc::ancestors;
use crate::reduce::default_account;
use crate::view::View;

/// A connection may send at most this much in total. Hook frames are scrubbed
/// of content and are a few hundred bytes.
pub const MAX_CONNECTION_BYTES: u64 = 1 << 20;

/// How far up the process tree to look for the agent a hook belongs to.
pub const ANCESTOR_LIMIT: usize = 20;

pub async fn serve(
    listener: UnixListener,
    inbox: mpsc::Sender<Input>,
    view: watch::Receiver<Arc<View>>,
    proc_root: PathBuf,
) {
    let proc_root: Arc<Path> = Arc::from(proc_root);
    loop {
        let stream = match listener.accept().await {
            Ok((stream, _)) => stream,
            Err(e) => {
                eprintln!("switchboard: accept failed: {e}");
                continue;
            }
        };
        let (read, write) = stream.into_split();
        tokio::spawn(handle(
            read,
            write,
            inbox.clone(),
            view.clone(),
            Arc::clone(&proc_root),
        ));
    }
}

/// Serve one connection until the peer closes it.
///
/// Generic over the two halves so a test can drive it with an in-memory
/// `tokio::io::duplex` pair instead of a socket.
pub async fn handle<R, W>(
    read: R,
    mut write: W,
    inbox: mpsc::Sender<Input>,
    mut view: watch::Receiver<Arc<View>>,
    proc_root: Arc<Path>,
) where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin,
{
    let mut lines = BufReader::new(read.take(MAX_CONNECTION_BYTES)).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        let request = match decode_request(&line) {
            Ok(request) => request,
            Err(e) => {
                if write_line(&mut write, &error_reply(&e)).await.is_err() {
                    return;
                }
                continue;
            }
        };
        let reply = match request {
            Request::Hook(frame) => {
                let process = resolve(&proc_root, frame.pid, &view).await;
                let ctx = Ctx {
                    now: now(),
                    process,
                };
                offer(&inbox, claude::hook(&frame, &ctx));
                ok_reply()
            }
            Request::Statusline(frame) => {
                let process = resolve(&proc_root, frame.pid, &view).await;
                let ctx = Ctx {
                    now: now(),
                    process,
                };
                offer(&inbox, claude::statusline(&frame, &default_account(), &ctx));
                ok_reply()
            }
            Request::List => snapshot_line(&view.borrow()),
            Request::Subscribe => {
                let first = snapshot_line(&view.borrow_and_update());
                if write_line(&mut write, &first).await.is_err() {
                    return;
                }
                while view.changed().await.is_ok() {
                    let next = snapshot_line(&view.borrow_and_update());
                    if write_line(&mut write, &next).await.is_err() {
                        return;
                    }
                }
                return;
            }
        };
        if write_line(&mut write, &reply).await.is_err() {
            return;
        }
    }
}

async fn write_line<W: AsyncWrite + Unpin>(write: &mut W, line: &str) -> std::io::Result<()> {
    write.write_all(line.as_bytes()).await?;
    write.write_all(b"\n").await?;
    write.flush().await
}

fn snapshot_line(view: &View) -> String {
    let snapshot = view.to_json(now(), SystemTime::now());
    serde_json::json!({ "snapshot": snapshot }).to_string()
}

/// Offer inputs without waiting. A full inbox means the core is behind; a
/// dropped hook is recovered by the transcript, a stalled agent is not.
fn offer(inbox: &mpsc::Sender<Input>, inputs: Vec<Input>) {
    use mpsc::error::TrySendError;

    for input in inputs {
        match inbox.try_send(input) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                eprintln!("switchboard: inbox full, dropping an input");
            }
            // The core is gone; the daemon is shutting down.
            Err(TrySendError::Closed(_)) => return,
        }
    }
}

/// The tracked agent a hook came from: the nearest ancestor of the
/// forwarder's parent that the board knows.
///
/// Reading `/proc` is blocking file I/O, so it runs on the blocking pool. The
/// closure must be `'static`, which is why it receives an owned `Arc` and an
/// owned copy of the tracked keys rather than borrowing either.
pub async fn resolve(
    proc_root: &Arc<Path>,
    pid: Option<u32>,
    view: &watch::Receiver<Arc<View>>,
) -> Option<ProcKey> {
    let pid = pid?;
    let tracked: Vec<ProcKey> = view.borrow().agents.iter().map(|a| a.process).collect();
    if tracked.is_empty() {
        return None;
    }
    let root = Arc::clone(proc_root);
    tokio::task::spawn_blocking(move || {
        ancestors(&root, pid, ANCESTOR_LIMIT)
            .find_map(|p| tracked.iter().find(|k| k.pid == p).copied())
    })
    .await
    .ok()
    .flatten()
}
