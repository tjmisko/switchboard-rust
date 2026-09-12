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
    todo!("T11")
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
    todo!("T11")
}

async fn write_line<W: AsyncWrite + Unpin>(write: &mut W, line: &str) -> std::io::Result<()> {
    todo!("T11")
}

fn snapshot_line(view: &View) -> String {
    let snapshot = view.to_json(now(), SystemTime::now());
    serde_json::json!({ "snapshot": snapshot }).to_string()
}

/// Offer inputs without waiting. A full inbox means the core is behind; a
/// dropped hook is recovered by the transcript, a stalled agent is not.
fn offer(inbox: &mpsc::Sender<Input>, inputs: Vec<Input>) {
    todo!("T11")
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
    todo!("T11")
}
