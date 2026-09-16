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
    let mut state: Option<(File, Tail)> = None;
    let mut confidence = Confidence::Inferred;

    loop {
        // A quiet transcript never sends, so a failed send would never tell
        // us the core is gone. Ask instead.
        if inbox.is_closed() {
            return;
        }
        ticker.tick().await;

        let taken = state.take();
        let path = spec.path.clone();
        let replay = spec.replay_bytes;
        let Ok((next, polled)) =
            tokio::task::spawn_blocking(move || poll_once(taken, &path, replay)).await
        else {
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
        for text in &polled.lines {
            let Ok(value) = serde_json::from_str::<Value>(text) else {
                continue;
            };
            for input in line(&value, &ctx) {
                if inbox.send(input).await.is_err() {
                    return;
                }
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
    let (mut file, mut tail) = match state {
        Some(open) => open,
        None => match open_at_tail(path, replay_bytes) {
            Ok(open) => open,
            // Not there yet, or unreadable: try again next tick.
            Err(_) => return (None, None),
        },
    };
    match tail.poll(&mut file) {
        Ok(polled) => (Some((file, tail)), Some(polled)),
        // Drop the handle and reopen next tick.
        Err(_) => (None, None),
    }
}

fn open_at_tail(path: &std::path::Path, replay_bytes: u64) -> io::Result<(File, Tail)> {
    let mut file = File::open(path)?;
    let start = tail_start(&mut file, replay_bytes)?;
    Ok((file, Tail::starting_at(start)))
}
