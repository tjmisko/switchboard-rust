//! Events: exact, lossy, applied once and in order.
//!
//! Every transition below is idempotent — a duplicate dispatch, a settle for a
//! call we never saw, a second `Stop` — because hooks are separate processes
//! and the transcript repeats what they said. A no-op returns `false` and
//! publishes nothing.

use std::time::{Instant, UNIX_EPOCH};

use crate::model::{
    Activity, Actor, Board, Compaction, Decision, Effect, Event, EventKind, Gate, GateKind,
    HistoryEntry, InputHash, Ownership, Prompts, StartSource, Subagent, SubagentId,
    SubagentSource, SubagentStatus, Subagents, Tool, ToolName, ToolOutcome, ToolUseId, TurnKey,
    TurnOutcome, Unjoined,
};
use crate::paths::workspace_name;
use crate::reduce::{Effects, limits_for, route, settle};

pub fn apply_event(board: &mut Board, event: Event) -> Vec<Effect> {
    todo!("T07")
}

/// Apply one event to one agent. Returns whether anything changed.
///
/// An event is applied to the actor it names: the first thing this does is
/// hand a subagent's event to `subagent_transition`. A subagent's turn is not
/// the agent's turn, so a child's `StopFailure` must never end the agent's
/// turn or paint it Blocked (47 of 56 captured failures were a child's).
fn transition(
    agent: &mut crate::model::Agent,
    event: &Event,
    now: Instant,
    effects: &mut Effects,
) -> bool {
    todo!("T07 (agent arms) and T08 (route subagent events, then the permission arms)")
}

/// Apply an event a subagent sent to that subagent's row. Returns whether
/// anything visible changed.
///
/// A child can legitimately report a turn ending, a tool dispatched or
/// settled, and a permission asked or settled. Its turn outcome is recorded
/// on the row: a failed child is a fact worth showing, and by R3 it may raise
/// the agent's phase only through a gate, never by itself. Everything else a
/// child could send says nothing about the agent and is dropped.
///
/// Every signal from a child refreshes the row's `last_seen`, which is what
/// keeps `tick` from expiring a child that is still working.
fn subagent_transition(
    agent: &mut crate::model::Agent,
    id: &SubagentId,
    event: &Event,
    now: Instant,
) -> bool {
    todo!("T08")
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

/// A prompt from the agent itself. Three paths: the call id is known, the
/// call is open and matches, or the prompt is parked.
///
/// Without a call id, a running call on the same `(tool, hash)` takes the
/// gate. A call *already blocked* on the same `(tool, hash)` means this is
/// the same prompt said twice: Claude sends `PreToolUse` (with the id) and
/// then `PermissionRequest` (without) for one question, 16-214 ms apart. The
/// id is the richer evidence and the hash must not contradict it, so the
/// second announcement is a duplicate, not a second question. Two identical
/// calls are still two gates: a running one is preferred.
fn ask(
    agent: &mut crate::model::Agent,
    call: Option<&ToolUseId>,
    tool: &ToolName,
    hash: Option<InputHash>,
    owner: Ownership,
    kind: GateKind,
    now: Instant,
) -> bool {
    todo!("T08")
}

/// A prompt from a subagent: its row is gated, or the prompt is parked under
/// that subagent until the row appears.
fn ask_subagent(
    agent: &mut crate::model::Agent,
    id: &SubagentId,
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
fn lift_subagent(agent: &mut crate::model::Agent, id: &SubagentId) -> bool {
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
