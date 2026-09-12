//! Events: exact, lossy, applied once and in order.
//!
//! Every transition below is idempotent — a duplicate dispatch, a settle for a
//! call we never saw, a second `Stop` — because hooks are separate processes
//! and the transcript repeats what they said. A no-op returns `false` and
//! publishes nothing.

use std::time::{Instant, UNIX_EPOCH};

use crate::model::{
    Activity, Actor, Board, Compaction, Decision, Effect, Event, EventKind, Gate, GateKind,
    HistoryEntry, InputHash, Ownership, Prompts, StartSource, Subagent, SubagentSource,
    SubagentStatus, Subagents, Tool, ToolName, ToolOutcome, ToolUseId, TurnKey, TurnOutcome,
    Unjoined,
};
use crate::paths::workspace_name;
use crate::reduce::{Effects, limits_for, route, settle};

pub fn apply_event(board: &mut Board, event: Event) -> Vec<Effect> {
    todo!("T07")
}

/// Apply one event to one agent. Returns whether anything changed.
fn transition(
    agent: &mut crate::model::Agent,
    event: &Event,
    now: Instant,
    effects: &mut Effects,
) -> bool {
    todo!("T07 (agent arms) and T08 (subagent and permission arms)")
}

fn idle(now: Instant, after: Option<TurnOutcome>) -> Activity {
    todo!("T07")
}

fn working(key: Option<TurnKey>, now: Instant) -> Activity {
    todo!("T07")
}

fn blocked(u: &Unjoined) -> Gate {
    todo!("T08")
}

/// Whether a prompt's hash admits a call's. A prompt that arrived without a
/// hash matches on name alone.
fn hash_admits(prompt: Option<InputHash>, call: InputHash) -> bool {
    todo!("T08")
}

fn end_turn(
    agent: &mut crate::model::Agent,
    turn: Option<&TurnKey>,
    outcome: &TurnOutcome,
    now: Instant,
) -> bool {
    todo!("T07")
}

fn dispatch(
    agent: &mut crate::model::Agent,
    call: &ToolUseId,
    tool: &ToolName,
    hash: InputHash,
    now: Instant,
) -> bool {
    todo!("T07")
}

/// A call finished, by id.
fn settle_call(agent: &mut crate::model::Agent, call: &ToolUseId, now: Instant) -> bool {
    todo!("T07")
}

#[allow(clippy::too_many_arguments)]
fn ask(
    agent: &mut crate::model::Agent,
    actor: &Actor,
    call: Option<&ToolUseId>,
    tool: &ToolName,
    hash: Option<InputHash>,
    owner: Ownership,
    kind: GateKind,
    now: Instant,
) -> bool {
    todo!("T08")
}

/// Hold a prompt that arrived before its call.
#[allow(clippy::too_many_arguments)]
fn park(
    prompts: &mut Prompts,
    actor: &Actor,
    tool: &ToolName,
    hash: Option<InputHash>,
    owner: Ownership,
    kind: GateKind,
    now: Instant,
) -> bool {
    todo!("T08")
}

/// Remove and return the oldest prompt this call satisfies.
fn take_unjoined(
    prompts: &mut Prompts,
    actor: &Actor,
    tool: &ToolName,
    hash: InputHash,
) -> Option<Unjoined> {
    todo!("T08")
}

/// Drop every unjoined prompt held by `actor`. Returns whether any were.
fn clear_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    todo!("T08")
}

fn lift_sole_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    todo!("T08")
}

/// Anything from a subagent that shows it is moving again lifts its gate.
fn lift_subagent(agent: &mut crate::model::Agent, id: &crate::model::SubagentId) -> bool {
    todo!("T08")
}

/// A permission prompt was answered.
fn answer(
    agent: &mut crate::model::Agent,
    call: Option<&ToolUseId>,
    outcome: Decision,
    now: Instant,
) -> bool {
    todo!("T08")
}
