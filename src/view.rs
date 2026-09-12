//! The published view of a board: what `state.json` and `subscribe` carry.
//!
//! Rendering is pure. A `View` holds `Instant`s so two renders of an unchanged
//! board compare equal, which is the publish gate; wall-clock times are only
//! computed when the view is serialised.

use std::path::PathBuf;
use std::time::{Instant, SystemTime};

use serde_json::{Value, json};

use crate::model::{
    Activity, Actor, Agent, Board, Gate, Lamp, LimitStatus, Limits, Phase, ProcKey, Reason,
    SessionId, SubagentStatus, TurnOutcome,
};
use crate::paths::display_name;
use crate::reduce::limits_for;
use crate::time::epoch_millis;

#[derive(Debug, Clone, PartialEq)]
pub struct View {
    pub agents: Vec<AgentView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AgentView {
    pub process: ProcKey,
    pub session: Option<SessionId>,
    pub name: String,
    pub cwd: PathBuf,
    /// What the chip shows.
    pub shown: Phase,
    pub since: Instant,
    /// The truth right now, which `shown` may lag.
    pub truth: Phase,
    pub reason: String,
    pub focused: bool,
}

impl View {
    pub fn render(board: &Board) -> View {
        let agents = board
            .agents
            .values()
            .map(|agent| {
                let limits = limits_for(&board.accounts, agent);
                AgentView {
                    process: agent.process,
                    session: agent.session.clone(),
                    name: display_name(agent).to_owned(),
                    cwd: agent.cwd.clone(),
                    shown: agent.shown.phase,
                    since: agent.shown.since,
                    truth: agent.phase(limits),
                    reason: describe(&reason(agent, limits)),
                    focused: agent.focused(),
                }
            })
            .collect();
        View { agents }
    }

    /// Serialise, converting `Instant`s to epoch milliseconds against one
    /// reading of each clock.
    pub fn to_json(&self, now: Instant, wall: SystemTime) -> Value {
        let to_wall = |at: Instant| {
            wall.checked_sub(now.saturating_duration_since(at))
                .unwrap_or(wall)
        };
        let agents: Vec<Value> = self
            .agents
            .iter()
            .map(|a| {
                json!({
                    "pid": a.process.pid,
                    "start": a.process.start,
                    "session_id": a.session.as_deref(),
                    "name": a.name,
                    "cwd": a.cwd.to_string_lossy(),
                    "phase": a.shown.as_str(),
                    "lamp": lamp_name(a.shown.lamp()),
                    "since_ms": epoch_millis(to_wall(a.since)),
                    "truth": a.truth.as_str(),
                    "reason": a.reason,
                    "focused": a.focused,
                })
            })
            .collect();
        json!({
            "schema": "switchboard-rust/1",
            "updated_ms": epoch_millis(wall),
            "agents": agents,
        })
    }
}

pub fn lamp_name(lamp: Lamp) -> &'static str {
    match lamp {
        Lamp::Grey => "grey",
        Lamp::Green => "green",
        Lamp::Amber => "amber",
        Lamp::Red => "red",
    }
}

/// Why an agent is in the phase it is in: the tooltip. Follows the same order
/// as `Agent::phase`, so the reason always explains the truth.
pub fn reason(agent: &Agent, limits: Option<&Limits>) -> Reason {
    if let Some(blocked) = human_gate(agent) {
        return blocked;
    }
    if let Some(l) = limits.filter(|l| l.status == LimitStatus::Rejected)
        && let Some(at) = l.resets_at
    {
        return Reason::LimitedUntil { at };
    }
    match &agent.activity {
        Activity::Unobserved { since } => Reason::NeverObserved { since: *since },
        Activity::Idle {
            after: Some(TurnOutcome::Failed { class }),
            ..
        } => Reason::TurnFailed { class: *class },
        Activity::Idle {
            after: Some(TurnOutcome::Interrupted { .. }),
            since,
        } => Reason::Interrupted { since: *since },
        Activity::Turn { tools, .. } => Reason::RunningTools { count: tools.len() },
        Activity::Idle { since, .. } => {
            let running = agent
                .subagents
                .rows
                .values()
                .filter(|s| matches!(s.status, SubagentStatus::Running | SubagentStatus::Pending))
                .count();
            if running > 0 {
                return Reason::SubagentsRunning { count: running };
            }
            if !agent.waits.is_empty() {
                return Reason::WaitsOutstanding {
                    count: agent.waits.len(),
                };
            }
            Reason::Idle { since: *since }
        }
    }
}

/// The first gate a person has to answer: the agent's own calls, then
/// unjoined prompts, then subagents.
fn human_gate(agent: &Agent) -> Option<Reason> {
    let own = match &agent.activity {
        Activity::Turn { tools, .. } => tools.values().find_map(|t| match t.gate {
            Gate::Blocked {
                kind,
                since,
                owner: crate::model::Ownership::Human,
                ..
            } => Some(Reason::Blocked {
                kind,
                tool: t.name.clone(),
                actor: Actor::Agent,
                since,
            }),
            _ => None,
        }),
        _ => None,
    };
    own.or_else(|| {
        agent
            .prompts
            .unjoined
            .iter()
            .find(|u| u.owner == crate::model::Ownership::Human)
            .map(|u| Reason::Blocked {
                kind: u.kind,
                tool: u.tool.clone(),
                actor: u.actor.clone(),
                since: u.since,
            })
    })
}

pub fn describe(reason: &Reason) -> String {
    match reason {
        Reason::Blocked {
            kind, tool, actor, ..
        } => {
            let who = match actor {
                Actor::Agent => String::new(),
                Actor::Subagent(id) => format!(" (subagent {id})"),
            };
            match kind {
                crate::model::GateKind::Approval => format!("wants approval to run {tool}{who}"),
                crate::model::GateKind::Question => format!("is asking a question via {tool}{who}"),
            }
        }
        Reason::NeverObserved { .. } => "not heard from yet".into(),
        Reason::LimitedUntil { at } => {
            format!("usage limit until {}", crate::time::format_rfc3339(*at))
        }
        Reason::TurnFailed { class } => format!("turn failed: {class:?}"),
        Reason::Interrupted { .. } => "stopped by you, work unfinished".into(),
        Reason::RunningTools { count: 0 } => "thinking".into(),
        Reason::RunningTools { count } => format!("running {count} tool call(s)"),
        Reason::SubagentsRunning { count } => format!("{count} subagent(s) running"),
        Reason::WaitsOutstanding { count } => format!("waiting on {count} background task(s)"),
        Reason::Idle { .. } => "idle".into(),
    }
}
