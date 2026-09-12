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

/// Run until the inbox closes. The effect runner ends a watcher by aborting
/// its task.
pub async fn run(spec: WatchSpec, inbox: mpsc::Sender<Input>) {
    let mut ticker = interval(spec.poll);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    // `None` until the file exists: a brand-new session's transcript appears
    // with its first line.
    let mut state: Option<(File, Tail)> = None;
    let mut confidence = Confidence::Inferred;

    loop {
        ticker.tick().await;
        // A quiet transcript never sends, so it would never learn the core
        // is gone by a failed send.
        if inbox.is_closed() {
            return;
        }

        let taken = state.take();
        let path = spec.path.clone();
        let replay = spec.replay_bytes;
        // The file and the cursor move into the blocking closure and come
        // back out with the result, so nothing is shared across the await.
        let joined = tokio::task::spawn_blocking(move || poll_once(taken, &path, replay)).await;
        let Ok((next, polled)) = joined else {
            return;
        };
        state = next;
        let Some(polled) = polled else {
            continue;
        };

        let ctx = LineCtx {
            process: spec.process,
            session: spec.session.clone(),
            account: spec.account.clone(),
            now: now(),
            confidence,
        };
        let inputs = polled
            .lines
            .iter()
            .filter_map(|text| serde_json::from_str::<Value>(text).ok())
            .flat_map(|v| line(&v, &ctx));
        for input in inputs {
            if inbox.send(input).await.is_err() {
                return;
            }
        }
        confidence = Confidence::Confirmed;
    }
}

/// One blocking step: open the file if needed, then read what is new.
/// Returns the state to keep, and what was read if anything could be.
fn poll_once(
    state: Option<(File, Tail)>,
    path: &std::path::Path,
    replay_bytes: u64,
) -> (Option<(File, Tail)>, Option<Polled>) {
    let opened = match state {
        Some(open) => Ok(open),
        None => open_at_tail(path, replay_bytes),
    };
    let Ok((mut file, mut tail)) = opened else {
        return (None, None);
    };
    match tail.poll(&mut file) {
        Ok(polled) => (Some((file, tail)), Some(polled)),
        // Keep the cursor; a transient read error is retried next tick.
        Err(_) => (Some((file, tail)), None),
    }
}

fn open_at_tail(path: &std::path::Path, replay_bytes: u64) -> io::Result<(File, Tail)> {
    let mut file = File::open(path)?;
    let start = tail_start(&mut file, replay_bytes)?;
    Ok((file, Tail::starting_at(start)))
}
