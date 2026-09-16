//! Events: exact, lossy, applied once and in order.
//!
//! Every transition below is idempotent — a duplicate dispatch, a settle for a
//! call we never saw, a second `Stop` — because hooks are separate processes
//! and the transcript repeats what they said. A no-op returns `false` and
//! publishes nothing.

use std::collections::BTreeMap;
use std::time::{Instant, UNIX_EPOCH};

use crate::model::{
    Activity, Actor, Board, Compaction, Decision, Effect, Event, EventKind, Gate, GateKind,
    HistoryEntry, InputHash, Ownership, Prompts, StartSource, Subagent, SubagentId,
    SubagentOutcome, SubagentSource, SubagentStatus, Subagents, Tool, ToolName, ToolOutcome,
    ToolUseId, TurnKey,
    TurnOutcome, Unjoined,
};
use crate::paths::workspace_name;
use crate::reduce::{Effects, limits_for, route, settle};

pub fn apply_event(board: &mut Board, event: Event) -> Vec<Effect> {
    let now = event.at.local;
    let mut effects = Effects::new();
    let Some(key) = route(board, event.process, &event.session) else {
        return effects.finish();
    };

    if matches!(event.kind, EventKind::Died) {
        if let Some(agent) = board.agents.remove(&key) {
            effects.push(Effect::AppendHistory(HistoryEntry::AgentGone {
                process: key,
                session: agent.session,
            }));
            effects.push(Effect::Forget { process: key });
        }
        return effects.finish();
    }

    let Board { agents, accounts } = board;
    let Some(agent) = agents.get_mut(&key) else {
        return effects.finish();
    };

    let mut changed = false;
    match &agent.session {
        // Discovered without a session: the first event names it.
        None => {
            agent.session = Some(event.session.clone());
            changed = true;
        }
        // A session the agent does not hold. `SessionStarted` is a rotation
        // (`/clear`); anything else is a late event for the old one.
        Some(current) if *current != event.session => {
            if !matches!(event.kind, EventKind::SessionStarted { .. }) {
                return effects.finish();
            }
            agent.session = Some(event.session.clone());
            agent.activity = idle(now, None);
            agent.prompts = Prompts::default();
            agent.compaction = Compaction::None;
            agent.waits.clear();
            agent.subagents = Subagents::default();
            changed = true;
        }
        Some(_) => {}
    }
    if changed {
        effects.push(Effect::WatchTranscript {
            process: agent.process,
            session: event.session.clone(),
            cwd: agent.cwd.clone(),
        });
    }

    changed |= transition(agent, &event, now, &mut effects);
    if changed {
        agent.last_event = Some(now);
    }
    let limits = limits_for(accounts, agent);
    effects.history(settle(agent, limits, event.cause, event.confidence, now));
    effects.publish_if(changed);
    effects.finish()
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
    if let Actor::Subagent(id) = &event.actor {
        return subagent_transition(agent, id, event, now);
    }
    match &event.kind {
        EventKind::SessionStarted { title, .. } => {
            let mut changed = false;
            if title.is_some() && agent.title != *title {
                agent.title = title.clone();
                changed = true;
            }
            if matches!(agent.activity, Activity::Unobserved { .. }) {
                agent.activity = idle(now, None);
                changed = true;
            }
            changed
        }
        // Not liveness: only `Died` ends an agent.
        EventKind::SessionEnded { .. } => false,
        EventKind::TurnStarted { turn, .. } => {
            if let Activity::Turn { key, .. } = &agent.activity
                && *key == *turn
            {
                return false;
            }
            // The next prompt lifts whatever the last one left parked (R4).
            clear_unjoined(&mut agent.prompts, &Actor::Agent);
            match &mut agent.activity {
                Activity::Turn { key, last_event, .. } => {
                    *key = turn.clone();
                    *last_event = now;
                }
                _ => agent.activity = working(turn.clone(), now),
            }
            true
        }
        EventKind::TurnEnded { turn, outcome } => end_turn(agent, turn.as_ref(), outcome, now),
        EventKind::ToolDispatched { call, tool, hash } => dispatch(agent, call, tool, *hash, now),
        EventKind::ToolSettled { call, outcome } => {
            let mut changed = settle_call(agent, call, now);
            if matches!(outcome, ToolOutcome::Failed { interrupt: true }) {
                let interrupted = TurnOutcome::Interrupted { killed: None };
                changed |= end_turn(agent, None, &interrupted, now);
            }
            changed
        }
        EventKind::CompactionStarted { trigger } => {
            if matches!(agent.compaction, Compaction::InProgress { .. }) {
                return false;
            }
            agent.compaction = Compaction::InProgress {
                since: now,
                trigger: *trigger,
            };
            true
        }
        EventKind::CompactionEnded => {
            if agent.compaction == Compaction::None {
                return false;
            }
            agent.compaction = Compaction::None;
            true
        }
        EventKind::CwdChanged { to } => {
            if agent.cwd == *to {
                return false;
            }
            agent.workspace = workspace_name(to).map(Box::from);
            agent.cwd = to.clone();
            true
        }
        // Handled in `apply_event` before the agent is borrowed.
        EventKind::Died => false,
        EventKind::PermissionAsked {
            call,
            tool,
            hash,
            owner,
            kind,
        } => ask(agent, call.as_ref(), tool, *hash, *owner, *kind, now),
        EventKind::PermissionSettled { call, outcome, .. } => {
            answer(agent, call.as_ref(), *outcome, now)
        }
        EventKind::SubagentStarted { subagent, kind } => {
            // The panel owns its table: events refine rows, never add them.
            if agent.subagents.source == SubagentSource::Panel {
                return false;
            }
            if agent.subagents.rows.contains_key(subagent) {
                return false;
            }
            agent.subagents.rows.insert(
                subagent.clone(),
                Subagent {
                    kind: *kind,
                    status: SubagentStatus::Running,
                    parent: None,
                    started: event.at.claimed.unwrap_or(UNIX_EPOCH),
                    label: subagent.as_str().into(),
                    tokens: 0,
                    gate: None,
                    last_seen: now,
                },
            );
            true
        }
        EventKind::SubagentEnded { subagent, outcome } => {
            // Whatever it left parked is not waiting on anyone now.
            let lifted = clear_unjoined(&mut agent.prompts, &Actor::Subagent(subagent.clone()));
            let Some(row) = agent.subagents.rows.get_mut(subagent) else {
                return lifted;
            };
            let status = match outcome {
                SubagentOutcome::Completed => SubagentStatus::Completed,
                SubagentOutcome::Failed => SubagentStatus::Failed,
                SubagentOutcome::Killed | SubagentOutcome::Interrupted => SubagentStatus::Killed,
            };
            row.last_seen = now;
            let ended = row.status != status || row.gate.is_some();
            row.status = status;
            row.gate = None;
            lifted || ended
        }
    }
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
    if let Some(row) = agent.subagents.rows.get_mut(id) {
        row.last_seen = now;
    }
    match &event.kind {
        EventKind::TurnEnded {
            outcome: TurnOutcome::Failed { .. },
            ..
        } => {
            let Some(row) = agent.subagents.rows.get_mut(id) else {
                return false;
            };
            if row.status == SubagentStatus::Failed && row.gate.is_none() {
                return false;
            }
            row.status = SubagentStatus::Failed;
            row.gate = None;
            true
        }
        EventKind::ToolDispatched { tool, hash, .. } => {
            let Some(row) = agent.subagents.rows.get_mut(id) else {
                return false;
            };
            if row.gate.is_some() {
                return false;
            }
            let Some(parked) = take_unjoined(&mut agent.prompts, &event.actor, tool, *hash) else {
                return false;
            };
            row.gate = Some(blocked(&parked));
            true
        }
        EventKind::ToolSettled { .. } | EventKind::PermissionSettled { .. } => {
            lift_subagent(agent, id) | lift_sole_unjoined(&mut agent.prompts, &event.actor)
        }
        EventKind::PermissionAsked {
            tool,
            hash,
            owner,
            kind,
            ..
        } => ask_subagent(agent, id, tool, *hash, *owner, *kind, now),
        _ => false,
    }
}

fn idle(now: Instant, after: Option<TurnOutcome>) -> Activity {
    Activity::Idle { since: now, after }
}

fn working(key: Option<TurnKey>, now: Instant) -> Activity {
    Activity::Turn {
        key,
        started: now,
        tools: BTreeMap::new(),
        last_event: now,
    }
}

fn blocked(u: &Unjoined) -> Gate {
    Gate::Blocked {
        kind: u.kind,
        since: u.since,
        owner: u.owner,
        req: None,
    }
}

/// Whether a prompt's hash admits a call's. A prompt that arrived without a
/// hash matches on name alone.
fn hash_admits(prompt: Option<InputHash>, call: InputHash) -> bool {
    prompt.is_none_or(|h| h == call)
}

fn end_turn(
    agent: &mut crate::model::Agent,
    turn: Option<&TurnKey>,
    outcome: &TurnOutcome,
    now: Instant,
) -> bool {
    // Both sides keyed and disagreeing: the late close of an earlier turn,
    // which says nothing about this one.
    if let Activity::Turn { key: Some(key), .. } = &agent.activity
        && turn.is_some_and(|t| t != key)
    {
        return false;
    }
    // Only the agent's own prompts: a subagent's gate lives on its row and a
    // subagent's parked prompt is keyed by its actor, so neither is touched.
    let lifted = clear_unjoined(&mut agent.prompts, &Actor::Agent);
    let ended = match &agent.activity {
        Activity::Turn { .. } => {
            agent.activity = idle(now, Some(outcome.clone()));
            true
        }
        // The hook's `Stop` and the transcript's interrupt line can land in
        // either order. An interrupt or failure after a plain completion is
        // still worth recording; a completion never erases one.
        Activity::Idle { after, .. } => {
            let plain = matches!(after, None | Some(TurnOutcome::Completed));
            if plain && *outcome != TurnOutcome::Completed {
                agent.activity = idle(now, Some(outcome.clone()));
                true
            } else {
                false
            }
        }
        // A replayed line is the first thing this agent ever said.
        Activity::Unobserved { .. } => {
            agent.activity = idle(now, Some(outcome.clone()));
            true
        }
    };
    lifted || ended
}

fn dispatch(
    agent: &mut crate::model::Agent,
    call: &ToolUseId,
    tool: &ToolName,
    hash: InputHash,
    now: Instant,
) -> bool {
    if !matches!(agent.activity, Activity::Turn { .. }) {
        agent.activity = working(None, now);
    }
    let Activity::Turn {
        tools, last_event, ..
    } = &mut agent.activity
    else {
        return false;
    };
    if tools.contains_key(call) {
        return false;
    }
    let gate = take_unjoined(&mut agent.prompts, &Actor::Agent, tool, hash)
        .map_or(Gate::Running, |u| blocked(&u));
    tools.insert(
        call.clone(),
        Tool {
            name: tool.clone(),
            hash,
            since: now,
            gate,
        },
    );
    *last_event = now;
    true
}

/// A call finished, by id.
fn settle_call(agent: &mut crate::model::Agent, call: &ToolUseId, now: Instant) -> bool {
    let Activity::Turn {
        tools, last_event, ..
    } = &mut agent.activity
    else {
        return lift_sole_unjoined(&mut agent.prompts, &Actor::Agent);
    };
    if tools.remove(call).is_none() {
        return lift_sole_unjoined(&mut agent.prompts, &Actor::Agent);
    }
    *last_event = now;
    true
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
    let gate = Gate::Blocked {
        kind,
        since: now,
        owner,
        req: None,
    };
    if let Some(call) = call {
        // The call is named. A question tool's `PreToolUse` is its own
        // dispatch, so the call may not exist yet.
        if !matches!(agent.activity, Activity::Turn { .. }) {
            agent.activity = working(None, now);
        }
        let Activity::Turn {
            tools, last_event, ..
        } = &mut agent.activity
        else {
            return false;
        };
        let entry = tools.entry(call.clone()).or_insert_with(|| Tool {
            name: tool.clone(),
            hash: hash.unwrap_or(InputHash([0; 8])),
            since: now,
            gate: Gate::Running,
        });
        if matches!(entry.gate, Gate::Blocked { .. }) {
            return false;
        }
        entry.gate = gate;
        *last_event = now;
        return true;
    }
    if let Activity::Turn {
        tools, last_event, ..
    } = &mut agent.activity
    {
        let matches = |t: &Tool| t.name == *tool && hash_admits(hash, t.hash);
        if let Some(running) = tools
            .values_mut()
            .find(|t| matches(t) && t.gate == Gate::Running)
        {
            running.gate = gate;
            *last_event = now;
            return true;
        }
        // The same prompt said twice: the id already gated this call.
        if tools.values().any(matches) {
            return false;
        }
    }
    park(&mut agent.prompts, &Actor::Agent, tool, hash, owner, kind, now)
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
    let Some(row) = agent.subagents.rows.get_mut(id) else {
        let actor = Actor::Subagent(id.clone());
        return park(&mut agent.prompts, &actor, tool, hash, owner, kind, now);
    };
    if matches!(row.gate, Some(Gate::Blocked { .. })) {
        return false;
    }
    row.gate = Some(Gate::Blocked {
        kind,
        since: now,
        owner,
        req: None,
    });
    true
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
    let duplicate = prompts
        .unjoined
        .iter()
        .any(|u| u.actor == *actor && u.tool == *tool && u.hash == hash);
    if duplicate {
        return false;
    }
    prompts.unjoined.push(Unjoined {
        actor: actor.clone(),
        tool: tool.clone(),
        hash,
        since: now,
        owner,
        kind,
    });
    true
}

/// Remove and return the oldest prompt this call satisfies.
fn take_unjoined(
    prompts: &mut Prompts,
    actor: &Actor,
    tool: &ToolName,
    hash: InputHash,
) -> Option<Unjoined> {
    let i = prompts
        .unjoined
        .iter()
        .position(|u| u.actor == *actor && u.tool == *tool && hash_admits(u.hash, hash))?;
    Some(prompts.unjoined.remove(i))
}

/// Drop every unjoined prompt held by `actor`. Returns whether any were.
fn clear_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    let before = prompts.unjoined.len();
    prompts.unjoined.retain(|u| u.actor != *actor);
    prompts.unjoined.len() != before
}

/// Lifts the one prompt `actor` has parked. Two or more is ambiguous, and
/// ambiguity holds the current state (R5).
fn lift_sole_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    let mut held = prompts
        .unjoined
        .iter()
        .enumerate()
        .filter(|(_, u)| u.actor == *actor)
        .map(|(i, _)| i);
    let (Some(i), None) = (held.next(), held.next()) else {
        return false;
    };
    prompts.unjoined.remove(i);
    true
}

/// Anything from a subagent that shows it is moving again lifts its gate.
fn lift_subagent(agent: &mut crate::model::Agent, id: &SubagentId) -> bool {
    agent
        .subagents
        .rows
        .get_mut(id)
        .and_then(|row| row.gate.take())
        .is_some()
}

/// The one blocked call, when exactly one is.
fn sole_blocked(tools: &BTreeMap<ToolUseId, Tool>) -> Option<ToolUseId> {
    let mut blocked = tools
        .iter()
        .filter(|(_, t)| matches!(t.gate, Gate::Blocked { .. }))
        .map(|(id, _)| id);
    match (blocked.next(), blocked.next()) {
        (Some(id), None) => Some(id.clone()),
        _ => None,
    }
}

/// A permission prompt was answered.
fn answer(
    agent: &mut crate::model::Agent,
    call: Option<&ToolUseId>,
    outcome: Decision,
    now: Instant,
) -> bool {
    let Activity::Turn {
        tools, last_event, ..
    } = &mut agent.activity
    else {
        return lift_sole_unjoined(&mut agent.prompts, &Actor::Agent);
    };
    // Without a call id, the answer can only be for the one blocked call.
    let Some(id) = call.cloned().or_else(|| sole_blocked(tools)) else {
        return lift_sole_unjoined(&mut agent.prompts, &Actor::Agent);
    };
    let Some(tool) = tools.get_mut(&id) else {
        return lift_sole_unjoined(&mut agent.prompts, &Actor::Agent);
    };
    match outcome {
        Decision::Allowed => {
            if tool.gate == Gate::Running {
                return false;
            }
            tool.gate = Gate::Running;
        }
        Decision::Denied { .. } | Decision::Abandoned => {
            tools.remove(&id);
        }
    }
    *last_event = now;
    true
}
