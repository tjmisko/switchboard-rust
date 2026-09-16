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

pub fn tick(board: &mut Board, now: Instant) -> Vec<Effect> {
    let mut effects = Effects::new();
    let Board { agents, accounts } = board;
    for agent in agents.values_mut() {
        let changed = expire(agent, now);
        let limits = limits_for(accounts, agent);
        effects.history(settle(agent, limits, Cause::Agent, Confidence::Inferred, now));
        effects.publish_if(changed);
    }
    effects.finish()
}

fn expire(agent: &mut Agent, now: Instant) -> bool {
    let mut changed = false;

    if stale_turn_deadline(agent).is_some_and(|deadline| now >= deadline) {
        // How it ended is unknown, which is not the same as `Completed`.
        agent.activity = Activity::Idle {
            since: now,
            after: None,
        };
        changed = true;
    }

    if let Compaction::InProgress { since, .. } = agent.compaction
        && now >= since + COMPACTION_STALE
    {
        agent.compaction = Compaction::None;
        changed = true;
    }

    let waits = agent.waits.len();
    agent.waits.retain(|_, wait| now < wait.expires);
    changed |= agent.waits.len() != waits;

    let prompts = agent.prompts.unjoined.len();
    agent
        .prompts
        .unjoined
        .retain(|prompt| now < prompt.since + UNJOINED_EXPIRY);
    changed |= agent.prompts.unjoined.len() != prompts;

    let rows = agent.subagents.rows.len();
    agent
        .subagents
        .rows
        .retain(|_, row| stale_subagent_deadline(row).is_none_or(|deadline| now < deadline));
    changed |= agent.subagents.rows.len() != rows;

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

/// When a silent subagent row would be forgotten, or `None` while it holds
/// a gate: a tick never clears a live gate, and the settle that answers it
/// refreshes `last_seen`. 54 of 168 captured children started and never
/// reported a stop; `expire` removes a row past this deadline whoever owns
/// the table, and `deadlines` includes it.
fn stale_subagent_deadline(row: &Subagent) -> Option<Instant> {
    let gated = matches!(row.gate, Some(Gate::Blocked { .. }));
    (!gated).then(|| row.last_seen + SUBAGENT_STALE)
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
        .map(|p| p.since + UNJOINED_EXPIRY);
    let rows = agent
        .subagents
        .rows
        .values()
        .filter_map(stale_subagent_deadline);
    pending
        .into_iter()
        .chain(turn)
        .chain(compaction)
        .chain(waits)
        .chain(prompts)
        .chain(rows)
}
