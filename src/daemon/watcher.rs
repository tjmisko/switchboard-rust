//! Following one agent's transcript.
//!
//! Polls the file's length rather than subscribing to inotify, because that
//! needs a syscall binding this crate does not depend on. At 200 ms the poll
//! is well inside the cost model's latency budget for the contradicted case
//! (an Escape the operator just pressed), which is what the budget protects.
//!
//! On first open it replays the last `replay_bytes` at
//! `Confidence::Inferred`: that is how an agent discovered mid-turn stops
//! being `Unobserved`. Everything after the first read is `Confirmed`.

use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};

use crate::daemon::now;
use crate::ingest::Input;
use crate::ingest::claude::{LineCtx, line};
use crate::model::{AccountId, Confidence, ProcKey, SessionId};
use crate::tail::{Polled, Tail, tail_start};

#[derive(Debug, Clone)]
pub struct WatchSpec {
    pub path: PathBuf,
    pub process: ProcKey,
    pub session: SessionId,
    pub account: AccountId,
    pub poll: Duration,
    pub replay_bytes: u64,
}

/// Run until the inbox closes, which is checked at the top of every pass: a
/// quiet transcript never sends, so a failed send alone would never notice.
/// The effect runner ends a watcher by aborting its task.
///
/// Each tick moves the open file and its cursor into a blocking closure and
/// takes them back with the result. The lines come back as text; parsing
/// them and handing them to the transcript adapter happens here, and a line
/// that is not JSON is skipped.
pub async fn run(spec: WatchSpec, inbox: mpsc::Sender<Input>) {
    todo!("T13")
}

/// One blocking step: open the file if needed, then read what is new.
/// Returns the state to keep, and what was read if anything could be.
fn poll_once(
    state: Option<(File, Tail)>,
    path: &std::path::Path,
    replay_bytes: u64,
) -> (Option<(File, Tail)>, Option<Polled>) {
    todo!("T13")
}

fn open_at_tail(path: &std::path::Path, replay_bytes: u64) -> io::Result<(File, Tail)> {
    todo!("T13")
}
