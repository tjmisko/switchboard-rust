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
    let proc_root: Arc<Path> = proc_root.into();
    loop {
        let Ok((stream, _addr)) = listener.accept().await else {
            continue;
        };
        let (read, write) = stream.into_split();
        tokio::spawn(handle(
            read,
            write,
            inbox.clone(),
            view.clone(),
            proc_root.clone(),
        ));
    }
}

/// Serve one connection until the peer closes it.
///
/// Generic over the two halves so a test can drive it with an in-memory
/// `tokio::io::duplex` pair instead of a socket.
///
/// `subscribe` is the last request a connection makes: from then on the
/// handler only writes, once now and once per view change, and ignores any
/// further lines. A peer that hangs up is noticed on the next write, which is
/// the next view change.
///
/// A `watch::Ref` from `view.borrow()` is not `Send`, so it must never live
/// across an `.await` here: take what is needed into a local first.
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
        let reply = match decode_request(&line) {
            Err(e) => error_reply(&e),

            Ok(Request::Hook(frame)) => {
                let process = resolve(&proc_root, frame.pid, &view).await;
                let ctx = Ctx {
                    now: now(),
                    process,
                };
                offer(&inbox, claude::hook(&frame, &ctx));
                ok_reply()
            }

            Ok(Request::Statusline(frame)) => {
                let process = resolve(&proc_root, frame.pid, &view).await;
                let ctx = Ctx {
                    now: now(),
                    process,
                };
                offer(&inbox, claude::statusline(&frame, &default_account(), &ctx));
                ok_reply()
            }

            Ok(Request::List) => snapshot_line(&view.borrow_and_update()),

            Ok(Request::Subscribe) => {
                // The last request on a connection: from here it only writes.
                loop {
                    let line = snapshot_line(&view.borrow_and_update());
                    if write_line(&mut write, &line).await.is_err() {
                        return;
                    }
                    if view.changed().await.is_err() {
                        return;
                    }
                }
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
    for input in inputs {
        if inbox.try_send(input).is_err() {
            eprintln!("switchboard: inbox full or closed, input dropped");
        }
    }
}

/// The tracked agent a hook came from: the nearest tracked ancestor of
/// `pid`, `pid` itself included. `HookFrame.pid` is the forwarder's parent,
/// and a hook runs as `claude -> sh -> switchboard-hook`, so the agent is
/// usually a step or two up.
///
/// Ancestors are matched by pid against the tracked keys, not by full
/// `ProcKey`: each step of the walk already reads one `stat`, and the tracked
/// set is at most one scan interval old, so a reused pid is corrected by the
/// next sweep's `Died`.
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
            .find_map(|p| tracked.iter().copied().find(|key| key.pid == p))
    })
    .await
    .ok()
    .flatten()
}
