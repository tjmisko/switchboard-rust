//! Time passing.
//!
//! A tick may end a turn that went quiet, forget a wait or a prompt past its
//! leak bound, and promote an escalation whose dwell has elapsed. It may
//! never clear a live gate or a limit: an agent parked on a permission prompt
//! has no CPU and no transcript growth, and looks exactly like a dead turn.

use std::time::Instant;

use crate::model::{Activity, Agent, Board, Cause, Compaction, Confidence, Effect, Gate};
use crate::reduce::{COMPACTION_STALE, Effects, TURN_STALE, UNJOINED_EXPIRY, limits_for, settle};

pub fn tick(board: &mut Board, now: Instant) -> Vec<Effect> {
    todo!("T09")
}

fn expire(agent: &mut Agent, now: Instant) -> bool {
    todo!("T09")
}

/// When a quiet turn would be declared over, or `None` if it cannot be:
/// not in a turn, a gate is open, or a compaction is running.
fn stale_turn_deadline(agent: &Agent) -> Option<Instant> {
    todo!("T09")
}

/// The earliest moment a `tick` could change anything. The core sleeps until
/// then instead of polling.
pub fn next_deadline(board: &Board) -> Option<Instant> {
    todo!("T09")
}

fn deadlines(agent: &Agent) -> impl Iterator<Item = Instant> + '_ {
    todo!("T09");
    #[allow(unreachable_code)]
    std::iter::empty()
}
