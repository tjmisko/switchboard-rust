//! The reducer: the only code that changes a `Board`.
//!
//! Three entry points, because three kinds of thing happen to a board and
//! they merge by different rules:
//!
//!   `apply_event`     something happened. Apply once, in order, by id.
//!   `apply_snapshot`  how something looked. Replace a whole scope, unless an
//!                     event has spoken since the snapshot was taken.
//!   `tick`            time passed. Expire what has a deadline.
//!
//! All three are pure and synchronous. They read no clock (time arrives in
//! the input), do no I/O (they return `Effect`s), and never block.
//!
//! After any change to an agent, the truth is recomputed and handed to
//! `Shown::settle`, which is the one place the cost model decides what the
//! operator sees.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use crate::ingest::Input;
use crate::model::{
    Account, AccountId, Agent, Board, Cause, Confidence, Effect, HistoryEntry, Limits, ProcKey,
    SessionId,
};

mod event;
mod snapshot;
mod tick;

pub use event::apply_event;
pub use snapshot::{apply_snapshot, discovered};
pub use tick::{next_deadline, tick};

/// A turn with no event for this long, and no gate open, is over.
pub const TURN_STALE: Duration = Duration::from_secs(10 * 60);
/// A compaction with no closing event for this long is over.
pub const COMPACTION_STALE: Duration = Duration::from_secs(10 * 60);
/// A background wait is forgotten this long after the last `Stop` reported
/// it. 55% of background tasks never write a closing record.
pub const WAIT_EXPIRY: Duration = Duration::from_secs(30 * 60);
/// An unjoined prompt is dropped this long after it arrived. A leak bound,
/// not a status rule: a real prompt is lifted long before by the call's
/// settle, the next prompt, or the end of the turn.
pub const UNJOINED_EXPIRY: Duration = Duration::from_secs(30 * 60);

/// The account every Claude agent bills against until a source names one.
pub fn default_account() -> AccountId {
    AccountId::from("default")
}

/// Apply either kind of input.
pub fn apply(board: &mut Board, input: Input) -> Vec<Effect> {
    match input {
        Input::Event(e) => apply_event(board, e),
        Input::Snapshot(s) => apply_snapshot(board, s),
    }
}

/// Which agent an input is about.
///
/// The process wins when it names a tracked agent. Otherwise the session is
/// searched for. Returns the key rather than `&mut Agent`: a `get_mut` in one
/// arm and a search in the other cannot both hold a mutable borrow of the map
/// across an `or_else`, but a `Copy` key can be found first and used after.
pub fn route(board: &Board, process: Option<ProcKey>, session: &SessionId) -> Option<ProcKey> {
    todo!("T07")
}

/// The limits an agent is subject to. Takes the accounts map rather than the
/// board, so it can be called while an agent in the same board is mutably
/// borrowed.
pub fn limits_for<'a>(
    accounts: &'a BTreeMap<AccountId, Account>,
    agent: &Agent,
) -> Option<&'a Limits> {
    todo!("T07")
}

/// Recompute the truth for one agent and let `Shown` decide whether the
/// operator sees it. Returns the history entry when the display moved.
pub fn settle(
    agent: &mut Agent,
    limits: Option<&Limits>,
    cause: Cause,
    confidence: Confidence,
    now: Instant,
) -> Option<HistoryEntry> {
    todo!("T07")
}

/// Collects effects and guarantees at most one `PublishState`, last.
#[derive(Debug, Default)]
pub struct Effects {
    list: Vec<Effect>,
    publish: bool,
}

impl Effects {
    pub fn new() -> Effects {
        Effects::default()
    }

    pub fn push(&mut self, effect: Effect) {
        match effect {
            Effect::PublishState => self.publish = true,
            Effect::AppendHistory(_) => {
                self.publish = true;
                self.list.push(effect);
            }
            other => self.list.push(other),
        }
    }

    pub fn history(&mut self, entry: Option<HistoryEntry>) {
        if let Some(entry) = entry {
            self.push(Effect::AppendHistory(entry));
        }
    }

    pub fn publish_if(&mut self, changed: bool) {
        self.publish |= changed;
    }

    pub fn finish(mut self) -> Vec<Effect> {
        if self.publish {
            self.list.push(Effect::PublishState);
        }
        self.list
    }
}
