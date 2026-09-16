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
    SubagentOutcome, SubagentSource, SubagentStatus, Subagents, Tool, ToolName, ToolOutcome,
    ToolUseId, TurnKey, TurnOutcome, Unjoined,
};
use crate::paths::workspace_name;
use crate::reduce::{Effects, limits_for, route, settle};

/// One event, in order:
///
/// 1. `route` it, or return nothing: agents exist because processes do.
/// 2. `Died` removes the agent, before the board is split.
/// 3. The `/clear` race. An agent that has rotated to a new session drops
///    every late event for the old one, and drops an event from its own
///    (tracked) pid that names any session it does not hold. Only a
///    `SessionStarted` may change which session that is. An agent with no
///    session yet adopts the event's.
/// 4. `transition`, then `last_event` is stamped if anything changed.
/// 5. `settle`, always, no-op or not. A duplicate is still silent, because
///    `Shown::settle` returns nothing when the truth already matches the
///    screen; and an escalation whose dwell has elapsed promotes on the
///    next event rather than waiting for a tick.
pub fn apply_event(board: &mut Board, event: Event) -> Vec<Effect> {
    todo!("T07")
}

/// Apply one event to one agent. Returns whether anything changed.
///
/// An event is applied to the actor it names: the first thing this does is
/// hand a subagent's event to `subagent_transition`. A subagent's turn is not
/// the agent's turn, so a child's `StopFailure` must never end the agent's
/// turn or paint it Blocked (47 of 56 captured failures were a child's).
///
/// The arms below are the agent's own. `SubagentStarted` and `SubagentEnded`
/// are among them: Claude reports a child's lifecycle through the *agent's*
/// hooks (`Actor::Agent`), so they never reach `subagent_transition`.
fn transition(
    agent: &mut crate::model::Agent,
    event: &Event,
    now: Instant,
    effects: &mut Effects,
) -> bool {
    todo!("T07a (session, compaction, cwd arms), T07b (turn and tool arms), T08 (route subagent events, then the permission and subagent arms)")
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

/// `Idle`, carrying how the turn that just ended did. Private to this
/// module: `tick.rs` writes the `Idle` literal itself.
fn idle(now: Instant, after: Option<TurnOutcome>) -> Activity {
    todo!("T07")
}

/// A fresh turn with no calls open. `key` is `None` for a turn a dispatch
/// started, because no prompt was seen.
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

/// The agent's turn ended, by whichever signal said so.
///
/// The agent's own parked prompts are lifted first, on every path, idle
/// included: the turn they belonged to is over. A subagent's parked prompts
/// are not touched (`Actor::Agent` is the filter), which is what keeps a
/// child's question alive through the parent's `Stop`.
///
/// Then, by activity:
///
/// - `Turn`: a key that names a different turn is ignored. Otherwise the
///   turn closes with `outcome`, abandoning its open calls.
/// - `Idle`: a second close may say *how* the turn ended, but never erases
///   that. A non-`Completed` outcome replaces `after: None | Completed`;
///   anything else is a no-op.
/// - `Unobserved`: goes idle with that outcome, whatever it is. A replayed
///   `Stop` is the first thing heard from this agent.
fn end_turn(
    agent: &mut crate::model::Agent,
    turn: Option<&TurnKey>,
    outcome: &TurnOutcome,
    now: Instant,
) -> bool {
    todo!("T07")
}

/// A call was dispatched. While idle (or unobserved) this starts a keyless
/// turn. A prompt parked for this `(tool, hash)` becomes the call's gate,
/// keeping the prompt's arrival time. The same call twice is a no-op.
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
    // T08. A placeholder so T07's `dispatch` compiles and runs: nothing is
    // ever parked before T08, so nothing is ever taken.
    None
}

/// Drop every unjoined prompt held by `actor`. Returns whether any were.
fn clear_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    // T08. A placeholder so T07's `end_turn` and `TurnStarted` can call it
    // from the right place now; nothing is parked before T08.
    false
}

fn lift_sole_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    // T08. A placeholder so T07's `settle_call` can call it now.
    false
}

/// Anything from a subagent that shows it is moving again lifts its gate.
fn lift_subagent(agent: &mut crate::model::Agent, id: &SubagentId) -> bool {
    todo!("T08")
}

/// A permission prompt was answered.
///
/// With a call id: `Allowed` releases that call's gate; `Denied` and
/// `Abandoned` drop the call. Without one (Claude's `PermissionDenied` may
/// carry none), the only thing it can mean is that a parked prompt was
/// answered: if exactly one of the agent's prompts is parked, it is lifted;
/// with none or several, nothing changes. A gated call is never released by
/// a call-less answer.
fn answer(
    agent: &mut crate::model::Agent,
    call: Option<&ToolUseId>,
    outcome: Decision,
    now: Instant,
) -> bool {
    todo!("T08")
}
