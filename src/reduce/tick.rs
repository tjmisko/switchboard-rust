//! Time passing.
//!
//! A tick may end a turn that went quiet, forget a wait or a prompt past its
//! leak bound, and promote an escalation whose dwell has elapsed. It may
//! never clear a live gate or a limit: an agent parked on a permission prompt
//! has no CPU and no transcript growth, and looks exactly like a dead turn.

use std::time::Instant;

use crate::model::{Activity, Agent, Board, Cause, Compaction, Confidence, Effect, Gate};
use crate::reduce::{
    COMPACTION_STALE, Effects, TURN_STALE, UNJOINED_EXPIRY, limits_for, settle,
};

pub fn tick(board: &mut Board, now: Instant) -> Vec<Effect> {
    let mut effects = Effects::new();
    let Board { agents, accounts } = board;

    for agent in agents.values_mut() {
        let changed = expire(agent, now);
        effects.publish_if(changed);
        let limits = limits_for(accounts, agent);
        effects.history(settle(agent, limits, Cause::Agent, Confidence::Inferred, now));
    }
    effects.finish()
}

fn expire(agent: &mut Agent, now: Instant) -> bool {
    let mut changed = false;

    if let Some(deadline) = stale_turn_deadline(agent) {
        if now >= deadline {
            agent.activity = Activity::Idle {
                since: now,
                after: None,
            };
            changed = true;
        }
    }

    let waits_before = agent.waits.len();
    agent.waits.retain(|_, w| w.expires > now);
    changed |= agent.waits.len() != waits_before;

    let prompts_before = agent.prompts.unjoined.len();
    agent
        .prompts
        .unjoined
        .retain(|u| u.since + UNJOINED_EXPIRY > now);
    changed |= agent.prompts.unjoined.len() != prompts_before;

    if let Compaction::InProgress { since, .. } = agent.compaction {
        if now >= since + COMPACTION_STALE {
            agent.compaction = Compaction::None;
            changed = true;
        }
    }
    changed
}

/// When a quiet turn would be declared over, or `None` if it cannot be:
/// not in a turn, a gate is open, or a compaction is running.
fn stale_turn_deadline(agent: &Agent) -> Option<Instant> {
    let Activity::Turn {
        tools, last_event, ..
    } = &agent.activity
    else {
        return None;
    };
    let gated = tools
        .values()
        .any(|t| matches!(t.gate, Gate::Blocked { .. }));
    let compacting = matches!(agent.compaction, Compaction::InProgress { .. });
    (!gated && !compacting).then(|| *last_event + TURN_STALE)
}

/// The earliest moment a `tick` could change anything. The core sleeps until
/// then instead of polling.
pub fn next_deadline(board: &Board) -> Option<Instant> {
    board.agents.values().flat_map(deadlines).min()
}

fn deadlines(agent: &Agent) -> impl Iterator<Item = Instant> + '_ {
    let pending = agent.shown.pending.as_ref().map(|e| e.promote_at);
    let turn = stale_turn_deadline(agent);
    let compaction = match agent.compaction {
        Compaction::InProgress { since, .. } => Some(since + COMPACTION_STALE),
        Compaction::None => None,
    };
    let waits = agent.waits.values().map(|w| w.expires);
    let prompts = agent
        .prompts
        .unjoined
        .iter()
        .map(|u| u.since + UNJOINED_EXPIRY);

    pending
        .into_iter()
        .chain(turn)
        .chain(compaction)
        .chain(waits)
        .chain(prompts)
}
