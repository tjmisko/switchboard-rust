//! The discovery sweep, and the death signal this build uses instead of a
//! pidfd.
//!
//! A process that was in the last sweep and is not in this one has died:
//! `ProcKey` carries the start time, so a recycled pid is a different key and
//! cannot hide a death. The sweep and the deaths it implies are sent from
//! this one task, in that order, so a sweep can never land after the death of
//! a process it contains.

use std::collections::BTreeSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::mpsc;
use tokio::time::{MissedTickBehavior, interval};

use crate::daemon::now;
use crate::ingest::Input;
use crate::model::{
    Actor, Cause, Confidence, Event, EventKind, Moment, ProcKey, ProcessRow, SessionId, Snapshot,
    SnapshotKind,
};
use crate::paths::Layout;
use crate::proc::sweep;

pub async fn run(layout: Arc<Layout>, inbox: mpsc::Sender<Input>, every: Duration) {
    let mut known = BTreeSet::new();
    let mut ticker = interval(every);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        ticker.tick().await;
        let layout = Arc::clone(&layout);
        let Ok(rows) =
            tokio::task::spawn_blocking(move || sweep(&layout.proc_root, &layout.claude_sessions))
                .await
        else {
            continue;
        };

        let at = now();
        let (inputs, seen) = sweep_inputs(&known, rows, at);
        for input in inputs {
            if inbox.send(input).await.is_err() {
                return;
            }
        }
        known = seen;
    }
}

/// One sweep's inputs: the snapshot, then a `Died` for every process that was
/// known before and is absent now. Also returns the new known set, which is
/// read off `rows` before `rows` moves into the snapshot.
pub fn sweep_inputs(
    known: &BTreeSet<ProcKey>,
    rows: Vec<ProcessRow>,
    at: Instant,
) -> (Vec<Input>, BTreeSet<ProcKey>) {
    todo!("T12")
}

fn died(process: ProcKey, at: Instant) -> Input {
    Input::Event(Event {
        at: Moment {
            local: at,
            claimed: None,
        },
        process: Some(process),
        // Routing is by process; a death is about the agent, whatever it was
        // talking about.
        session: SessionId::from(""),
        actor: Actor::Agent,
        cause: Cause::Agent,
        confidence: Confidence::Confirmed,
        kind: EventKind::Died,
    })
}
