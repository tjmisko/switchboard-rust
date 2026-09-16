//! Events: exact, lossy, applied once and in order.
//!
//! Every transition below is idempotent — a duplicate dispatch, a settle for a
//! call we never saw, a second `Stop` — because hooks are separate processes
//! and the transcript repeats what they said. A no-op returns `false` and
//! publishes nothing.

use std::time::{Instant, UNIX_EPOCH};

use crate::model::{
    Activity, Actor, Board, Compaction, Decision, Effect, Event, EventKind, Gate, GateKind,
    HistoryEntry, InputHash, Ownership, Prompts, StartSource, Subagent, SubagentId, SubagentSource,
    SubagentStatus, Subagents, Tool, ToolName, ToolOutcome, ToolUseId, TurnKey, TurnOutcome,
    Unjoined,
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
    // `limits` borrows `accounts`, not `agent`: see `limits_for`.
    let limits = limits_for(accounts, agent);
    effects.history(settle(agent, limits, event.cause, event.confidence, now));
    effects.finish()
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
    // An event is applied to the actor it names. A subagent's turn is not
    // the agent's turn: Claude reports a child's lifecycle through
    // SubagentStart/Stop, and a child's own hooks refine that child's row
    // only. Without this, a child's `StopFailure` ends the parent's turn and
    // paints the parent Blocked (47 of 56 captured failures were a child's).
    if let Actor::Subagent(id) = &event.actor {
        return subagent_transition(agent, id, event, now);
    }

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
            // The duplicate check comes first. The hook and the transcript
            // both report one prompt, and a `PermissionRequest` can land
            // between them; the second report must not lift what the first
            // parked.
            if let Activity::Turn { key, .. } = &agent.activity
                && turn.is_some()
                && key == turn
            {
                return false;
            }
            // A new prompt answers whatever the agent was asking, idle or
            // not. Its parked prompts are the agent's own; a subagent's stay.
            clear_unjoined(&mut agent.prompts, &Actor::Agent);
            match &mut agent.activity {
                Activity::Turn {
                    key, last_event, ..
                } => {
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

        EventKind::ToolDispatched { call, tool, hash } => dispatch(agent, call, tool, *hash, now),

        EventKind::ToolSettled { call, outcome } => {
            let changed = settle_call(agent, call, now);
            // Escape during a tool ends the turn, and the operator knows.
            if let ToolOutcome::Failed { interrupt: true } = outcome {
                let ended = end_turn(agent, None, &TurnOutcome::Interrupted { killed: None }, now);
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
        } => ask(agent, call.as_ref(), tool, *hash, *owner, *kind, now),

        EventKind::PermissionSettled { call, outcome, .. } => {
            answer(agent, call.as_ref(), *outcome, now)
        }

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
            // Two lifecycles, by who owns the table. Under `Panel` the panel
            // owns rows: an event may refine one (status, gate), never add or
            // remove it. Under `EventsOnly` the events own them: `Started`
            // created the row, so `Ended` removes it and drops any prompt the
            // child left parked; a child that never reports a stop is
            // forgotten by `tick` after `SUBAGENT_STALE`.
            if agent.subagents.source == SubagentSource::Panel {
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
    let actor = &event.actor;
    if let Some(row) = agent.subagents.rows.get_mut(id) {
        row.last_seen = now;
    }

    match &event.kind {
        EventKind::TurnEnded {
            outcome: TurnOutcome::Failed { .. },
            ..
        } => {
            let cleared = clear_unjoined(&mut agent.prompts, actor);
            let failed = match agent.subagents.rows.get_mut(id) {
                Some(row) if row.status != SubagentStatus::Failed => {
                    row.status = SubagentStatus::Failed;
                    row.gate = None;
                    true
                }
                _ => false,
            };
            cleared || failed
        }

        EventKind::ToolDispatched { tool, hash, .. } => {
            let Some(row) = agent.subagents.rows.get_mut(id) else {
                return false;
            };
            match take_unjoined(&mut agent.prompts, actor, tool, *hash) {
                Some(u) => {
                    row.gate = Some(blocked(&u));
                    true
                }
                None => false,
            }
        }

        EventKind::ToolSettled { .. } | EventKind::PermissionSettled { .. } => {
            lift_subagent(agent, id)
        }

        EventKind::PermissionAsked {
            tool,
            hash,
            owner,
            kind,
            ..
        } => ask_subagent(agent, id, tool, *hash, *owner, *kind, now),

        // A child's turn starting or completing, its session, its compaction
        // and its cwd are its own business; SubagentStart/Stop carry what
        // the agent needs to know.
        EventKind::TurnEnded { .. }
        | EventKind::TurnStarted { .. }
        | EventKind::SessionStarted { .. }
        | EventKind::SessionEnded { .. }
        | EventKind::SubagentStarted { .. }
        | EventKind::SubagentEnded { .. }
        | EventKind::CompactionStarted { .. }
        | EventKind::CompactionEnded
        | EventKind::CwdChanged { .. }
        | EventKind::Died => false,
    }
}

/// `Idle`, carrying how the turn that just ended did. Private to this
/// module: `tick.rs` writes the `Idle` literal itself.
fn idle(now: Instant, after: Option<TurnOutcome>) -> Activity {
    Activity::Idle { since: now, after }
}

/// A fresh turn with no calls open. `key` is `None` for a turn a dispatch
/// started, because no prompt was seen.
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
    clear_unjoined(&mut agent.prompts, &Actor::Agent);
    match &agent.activity {
        Activity::Turn { key, .. } => {
            if let (Some(open), Some(closing)) = (key, turn)
                && open != closing
            {
                return false;
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

/// A prompt from the agent itself. Three paths: the call id is known, the
/// call is open and matches, or the prompt is parked.
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

    // Without one, join on (tool, hash). A running call takes the gate. A
    // call already blocked on the same (tool, hash) means this is the same
    // prompt said twice: Claude sends `PreToolUse` (with the id) and then
    // `PermissionRequest` (without) for one question, 16-214 ms apart. The
    // id is the richer evidence and the hash must not contradict it, so the
    // second announcement is a duplicate, not a second question. Two
    // identical calls are still two gates: a running one is preferred.
    if let Activity::Turn { tools, .. } = &mut agent.activity {
        let same = |t: &Tool| t.name == *tool && hash_admits(hash, t.hash);
        let open = tools
            .values_mut()
            .find(|t| same(t) && matches!(t.gate, Gate::Running));
        if let Some(t) = open {
            t.gate = gate;
            return true;
        }
        let already_gated = tools
            .values()
            .any(|t| same(t) && matches!(t.gate, Gate::Blocked { .. }));
        if already_gated {
            return false;
        }
    }
    park(
        &mut agent.prompts,
        &Actor::Agent,
        tool,
        hash,
        owner,
        kind,
        now,
    )
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
    let actor = Actor::Subagent(id.clone());
    let Some(row) = agent.subagents.rows.get_mut(id) else {
        return park(&mut agent.prompts, &actor, tool, hash, owner, kind, now);
    };
    let already = matches!(row.gate, Some(Gate::Blocked { .. }));
    row.gate = Some(Gate::Blocked {
        kind,
        since: now,
        owner,
        req: None,
    });
    !already
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
fn lift_subagent(agent: &mut crate::model::Agent, id: &SubagentId) -> bool {
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
