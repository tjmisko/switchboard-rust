//! Time passing.
//!
//! A tick may end a turn that went quiet, forget a wait, a prompt or a
//! subagent row past its leak bound, and promote an escalation whose dwell
//! has elapsed. It may never clear a live gate or a limit: an agent parked on
//! a permission prompt has no CPU and no transcript growth, and looks exactly
//! like a dead turn.

use std::time::Instant;

use crate::model::{
    Activity, Agent, Board, Cause, Compaction, Confidence, Effect, Gate, Subagent,
};
use crate::reduce::{
    COMPACTION_STALE, Effects, SUBAGENT_STALE, TURN_STALE, UNJOINED_EXPIRY, limits_for, settle,
};

/// Time passed. Every agent is expired, then re-settled with `Cause::Agent`
/// and `Confidence::Inferred`: nothing a tick learns was confirmed by
/// anyone, so an escalation it uncovers waits its dwell like any other
/// inferred one. The same settle is what promotes a pending escalation
/// whose deadline has arrived, which is why the core ticks at
/// `next_deadline` rather than on a timer.
pub fn tick(board: &mut Board, now: Instant) -> Vec<Effect> {
    todo!("T09")
}

/// Everything with a deadline that has passed, in one agent. Returns
/// whether anything changed.
///
/// A stale turn closes to `Idle { after: None }`: how it ended is unknown,
/// so no outcome is claimed. `Activity::Idle` is written as a literal here
/// (`event::idle` is private to its module). Waits, parked prompts and
/// subagent rows past their bounds are dropped; a compaction past
/// `COMPACTION_STALE` is over.
fn expire(agent: &mut Agent, now: Instant) -> bool {
    todo!("T09")
}

/// When a quiet turn would be declared over, or `None` if it cannot be:
/// not in a turn, a gate is open, or a compaction is running.
fn stale_turn_deadline(agent: &Agent) -> Option<Instant> {
    todo!("T09")
}

/// When a silent subagent row would be forgotten, or `None` while it holds
/// a gate: a tick never clears a live gate, and the settle that answers it
/// refreshes `last_seen`. 54 of 168 captured children started and never
/// reported a stop; `expire` removes a row past this deadline whoever owns
/// the table, and `deadlines` includes it.
fn stale_subagent_deadline(row: &Subagent) -> Option<Instant> {
    todo!("T09")
}

/// The earliest moment a `tick` could change anything. The core sleeps until
/// then instead of polling.
pub fn next_deadline(board: &Board) -> Option<Instant> {
    todo!("T09")
}

fn deadlines(agent: &Agent) -> impl Iterator<Item = Instant> + '_ {
    // T09. Replace this whole body. The `empty()` below only keeps the
    // `impl Iterator` signature compiling until you do; it is not a start.
    todo!("T09");
    #[allow(unreachable_code)]
    std::iter::empty()
}
