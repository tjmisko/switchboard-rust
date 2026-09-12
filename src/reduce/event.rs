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
    let mut effects = Effects::new();
    let Some(key) = route(board, event.process, &event.session) else {
        // A hook naming no tracked agent creates nothing: agents exist
        // because processes exist.
        return effects.finish();
    };
    let now = event.at.local;

    if let EventKind::Died = event.kind {
        if let Some(gone) = board.agents.remove(&key) {
            effects.push(Effect::Forget { process: key });
            effects.push(Effect::AppendHistory(HistoryEntry::AgentGone {
                process: key,
                session: gone.session,
            }));
        }
        return effects.finish();
    }

    // Split the board so an agent can be mutated while its account is read.
    let Board { agents, accounts } = board;
    let Some(agent) = agents.get_mut(&key) else {
        return effects.finish();
    };

    // The `/clear` race: once an agent has moved to a new conversation, late
    // events for the old one say nothing about it.
    let starting = matches!(event.kind, EventKind::SessionStarted { .. });
    match &agent.session {
        Some(current) if *current != event.session && !starting => return effects.finish(),
        None if !starting => {
            agent.session = Some(event.session.clone());
            effects.push(Effect::WatchTranscript {
                process: key,
                session: event.session.clone(),
                cwd: agent.cwd.clone(),
            });
        }
        _ => {}
    }

    let changed = transition(agent, &event, now, &mut effects);
    if changed {
        agent.last_event = Some(now);
    }
    effects.publish_if(changed);
    effects.history(settle(
        agent,
        limits_for(accounts, agent),
        event.cause,
        event.confidence,
        now,
    ));
    effects.finish()
}

/// Apply one event to one agent. Returns whether anything changed.
fn transition(
    agent: &mut crate::model::Agent,
    event: &Event,
    now: Instant,
    effects: &mut Effects,
) -> bool {
    match &event.kind {
        EventKind::SessionStarted { source, title } => {
            if let Some(title) = title {
                agent.title = Some(title.clone());
            }
            let rotated = agent.session.as_ref() != Some(&event.session);
            if rotated || *source == StartSource::Clear {
                // A new conversation: nothing the old one had open survives.
                agent.activity = idle(now, None);
                agent.prompts = Prompts::default();
                agent.subagents = Subagents::default();
                agent.waits.clear();
                agent.compaction = Compaction::None;
            }
            if rotated {
                agent.session = Some(event.session.clone());
                effects.push(Effect::WatchTranscript {
                    process: agent.process,
                    session: event.session.clone(),
                    cwd: agent.cwd.clone(),
                });
            }
            if let Activity::Unobserved { .. } = agent.activity {
                agent.activity = idle(now, None);
            }
            true
        }

        // Not liveness, and `/clear` sends one just before its SessionStart.
        EventKind::SessionEnded { .. } => false,

        EventKind::TurnStarted { turn, .. } => {
            clear_unjoined(&mut agent.prompts, &Actor::Agent);
            match &mut agent.activity {
                Activity::Turn { key, last_event, .. } => {
                    if turn.is_some() && key == turn {
                        return false;
                    }
                    // A queued prompt dequeued mid-turn: same turn of work,
                    // new key. Open calls are still running.
                    *key = turn.clone();
                    *last_event = now;
                }
                _ => agent.activity = working(turn.clone(), now),
            }
            true
        }

        EventKind::TurnEnded { turn, outcome } => end_turn(agent, turn.as_ref(), outcome, now),

        EventKind::ToolDispatched { call, tool, hash } => match &event.actor {
            Actor::Subagent(id) => {
                let Some(row) = agent.subagents.rows.get_mut(id) else {
                    return false;
                };
                row.last_seen = now;
                if let Some(u) = take_unjoined(&mut agent.prompts, &event.actor, tool, *hash) {
                    row.gate = Some(blocked(&u));
                }
                true
            }
            Actor::Agent => dispatch(agent, call, tool, *hash, now),
        },

        EventKind::ToolSettled { call, outcome } => {
            let changed = match &event.actor {
                Actor::Subagent(id) => lift_subagent(agent, id),
                Actor::Agent => settle_call(agent, call, now),
            };
            // Escape during a tool ends the turn, and the operator knows.
            if let ToolOutcome::Failed { interrupt: true } = outcome {
                let ended = end_turn(
                    agent,
                    None,
                    &TurnOutcome::Interrupted { killed: None },
                    now,
                );
                return changed || ended;
            }
            changed
        }

        EventKind::PermissionAsked {
            call,
            tool,
            hash,
            owner,
            kind,
        } => ask(agent, &event.actor, call.as_ref(), tool, *hash, *owner, *kind, now),

        EventKind::PermissionSettled { call, outcome, .. } => match &event.actor {
            Actor::Subagent(id) => lift_subagent(agent, id),
            Actor::Agent => answer(agent, call.as_ref(), *outcome, now),
        },

        EventKind::SubagentStarted { subagent, kind } => {
            if agent.subagents.source == SubagentSource::Panel
                || agent.subagents.rows.contains_key(subagent)
            {
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

        EventKind::SubagentEnded { subagent, .. } => {
            if agent.subagents.source == SubagentSource::Panel {
                // The panel owns rows. An event may refine one, never remove it.
                return match agent.subagents.rows.get_mut(subagent) {
                    Some(row) if row.status != SubagentStatus::Completed => {
                        row.status = SubagentStatus::Completed;
                        row.gate = None;
                        true
                    }
                    _ => false,
                };
            }
            let removed = agent.subagents.rows.remove(subagent).is_some();
            let actor = Actor::Subagent(subagent.clone());
            let cleared = clear_unjoined(&mut agent.prompts, &actor);
            removed || cleared
        }

        EventKind::CompactionStarted { trigger } => {
            agent.compaction = Compaction::InProgress {
                since: now,
                trigger: *trigger,
            };
            true
        }

        EventKind::CompactionEnded => {
            let was = agent.compaction != Compaction::None;
            agent.compaction = Compaction::None;
            was
        }

        EventKind::CwdChanged { to } => {
            if agent.cwd == *to {
                return false;
            }
            agent.workspace = workspace_name(to).map(Box::from);
            agent.cwd = to.clone();
            true
        }

        // Handled before the agent is borrowed.
        EventKind::Died => false,
    }
}

fn idle(now: Instant, after: Option<TurnOutcome>) -> Activity {
    Activity::Idle { since: now, after }
}

fn working(key: Option<TurnKey>, now: Instant) -> Activity {
    Activity::Turn {
        key,
        started: now,
        tools: Default::default(),
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
    clear_unjoined(&mut agent.prompts, &Actor::Agent);
    match &agent.activity {
        Activity::Turn { key, .. } => {
            if let (Some(open), Some(closing)) = (key, turn) {
                if open != closing {
                    return false;
                }
            }
            // Open calls are abandoned with the turn.
            agent.activity = idle(now, Some(outcome.clone()));
            true
        }
        Activity::Idle { after, .. } => {
            // A second close of a finished turn may say *how* it ended — an
            // interrupt line landing after `Stop` — but never erases that.
            let informative = !matches!(outcome, TurnOutcome::Completed);
            let already_explained = matches!(
                after,
                Some(TurnOutcome::Interrupted { .. } | TurnOutcome::Failed { .. })
            );
            if informative && !already_explained {
                agent.activity = idle(now, Some(outcome.clone()));
                return true;
            }
            false
        }
        Activity::Unobserved { .. } => {
            agent.activity = idle(now, Some(outcome.clone()));
            true
        }
    }
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
    let gate = take_unjoined(&mut agent.prompts, &Actor::Agent, tool, hash)
        .map_or(Gate::Running, |u| blocked(&u));

    let Activity::Turn {
        tools, last_event, ..
    } = &mut agent.activity
    else {
        unreachable!("a turn was just ensured");
    };
    *last_event = now;
    if tools.contains_key(call) {
        return false;
    }
    tools.insert(
        call.clone(),
        Tool {
            name: tool.clone(),
            hash,
            since: now,
            gate,
        },
    );
    true
}

/// A call finished, by id.
fn settle_call(agent: &mut crate::model::Agent, call: &ToolUseId, now: Instant) -> bool {
    if let Activity::Turn {
        tools, last_event, ..
    } = &mut agent.activity
    {
        *last_event = now;
        if tools.remove(call).is_some() {
            return true;
        }
    }
    // A call we never saw dispatched. If exactly one prompt is waiting to be
    // joined, this is almost certainly its call finishing — and lifting a
    // stale alert is the cheap mistake.
    lift_sole_unjoined(&mut agent.prompts, &Actor::Agent)
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
    let gate = Gate::Blocked {
        kind,
        since: now,
        owner,
        req: None,
    };

    if let Actor::Subagent(id) = actor {
        if let Some(row) = agent.subagents.rows.get_mut(id) {
            let already = matches!(row.gate, Some(Gate::Blocked { .. }));
            row.gate = Some(gate);
            row.last_seen = now;
            return !already;
        }
        return park(&mut agent.prompts, actor, tool, hash, owner, kind, now);
    }

    // With a call id, the gate goes straight onto the call — creating it if
    // this is a question tool, whose ask is its own dispatch.
    if let Some(call) = call {
        if !matches!(agent.activity, Activity::Turn { .. }) {
            agent.activity = working(None, now);
        }
        let Activity::Turn {
            tools, last_event, ..
        } = &mut agent.activity
        else {
            unreachable!("a turn was just ensured");
        };
        *last_event = now;
        let entry = tools.entry(call.clone()).or_insert_with(|| Tool {
            name: tool.clone(),
            hash: hash.unwrap_or(InputHash([0; 8])),
            since: now,
            gate: Gate::Running,
        });
        let already = matches!(entry.gate, Gate::Blocked { .. });
        entry.gate = gate;
        return !already;
    }

    // Without one, join on (tool, hash) against a call that is running.
    if let Activity::Turn { tools, .. } = &mut agent.activity {
        let open = tools.values_mut().find(|t| {
            t.name == *tool && matches!(t.gate, Gate::Running) && hash_admits(hash, t.hash)
        });
        if let Some(t) = open {
            t.gate = gate;
            return true;
        }
    }
    park(&mut agent.prompts, actor, tool, hash, owner, kind, now)
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

fn lift_sole_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    let mut held = prompts.unjoined.iter().filter(|u| u.actor == *actor);
    match (held.next(), held.next()) {
        (Some(_), None) => clear_unjoined(prompts, actor),
        _ => false,
    }
}

/// Anything from a subagent that shows it is moving again lifts its gate.
fn lift_subagent(agent: &mut crate::model::Agent, id: &crate::model::SubagentId) -> bool {
    let actor = Actor::Subagent(id.clone());
    let cleared = clear_unjoined(&mut agent.prompts, &actor);
    let lifted = agent
        .subagents
        .rows
        .get_mut(id)
        .and_then(|row| row.gate.take())
        .is_some();
    cleared || lifted
}

/// A permission prompt was answered.
fn answer(
    agent: &mut crate::model::Agent,
    call: Option<&ToolUseId>,
    outcome: Decision,
    now: Instant,
) -> bool {
    let Some(call) = call else {
        return lift_sole_unjoined(&mut agent.prompts, &Actor::Agent);
    };
    let Activity::Turn {
        tools, last_event, ..
    } = &mut agent.activity
    else {
        return false;
    };
    *last_event = now;
    match outcome {
        Decision::Allowed => match tools.get_mut(call) {
            Some(tool) if matches!(tool.gate, Gate::Blocked { .. }) => {
                tool.gate = Gate::Running;
                true
            }
            _ => false,
        },
        Decision::Denied { .. } | Decision::Abandoned => tools.remove(call).is_some(),
    }
}
