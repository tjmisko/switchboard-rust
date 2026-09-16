//! Claude hook frames -> inputs.
//!
//! The mapping, from the Ingestion tab of the signals guide:
//!
//! | hook                     | input                                   | cause            |
//! |--------------------------|-----------------------------------------|------------------|
//! | SessionStart{source}     | SessionStarted                          | User if `clear`  |
//! | UserPromptSubmit         | TurnStarted{prompt_id}                  | User only when `source` says human |
//! | PreToolUse               | ToolDispatched, or PermissionAsked{Question} for a question tool | Agent |
//! | PermissionRequest        | PermissionAsked{call: None}             | Agent            |
//! | PermissionDenied         | PermissionSettled{Denied}               | Agent            |
//! | PostToolUse              | ToolSettled{Ok}                         | Agent            |
//! | PostToolUseFailure       | ToolSettled{Failed}                     | User if interrupt |
//! | Stop                     | TurnEnded{Completed} + Snapshot::Waits  | Agent            |
//! | StopFailure{error}       | TurnEnded{Failed{class}}                | Agent            |
//! | SubagentStart / Stop     | SubagentStarted / Ended                 | Agent            |
//! | PreCompact / PostCompact | CompactionStarted / Ended               | User if `manual` |
//! | CwdChanged{new_cwd}      | CwdChanged                              | Agent            |
//! | SessionEnd{reason}       | SessionEnded                            | Agent            |
//! | Elicitation              | PermissionAsked{Question}               | Agent            |
//!
//! Every hook is `Confidence::Confirmed`: it is the harness saying so.
//!
//! Every event's actor is the subagent `agent_id` names, when present. That
//! is the common case, not the exception: on a machine that delegates, 97%
//! of tool hooks and most `StopFailure`s come from a child. The reducer
//! applies each event to the actor it names, so a child's failed turn lands
//! on the child's row and never on the agent's activity.
//!
//! Two documented fields have never been captured on 2.1.269/2.1.270 and are
//! read only as tolerance for other builds: `SessionStart.session_title`
//! (the name comes from the session file instead, see `proc::name_from_file`)
//! and `UserPromptSubmit.source` (absent means the origin is unknown, not
//! that a person typed it).

use std::path::PathBuf;

use serde_json::Value;

use crate::ingest::json::{array_at, bool_at, nonblank_str_at, str_at, u64_at};
use crate::ingest::{Ctx, Input, frame::HookFrame, hash::input_hash};
use crate::model::{
    Actor, Cause, Confidence, Decision, Denier, ErrorClass, Event, EventKind, GateKind, InputHash,
    Moment, Origin, Ownership, PromptId, Snapshot, SnapshotKind, StartSource, SubagentId,
    SubagentKind, SubagentOutcome, ToolName, ToolOutcome, ToolUseId, Trigger, TurnKey,
    TurnOutcome, WaitId, WaitKind, WaitRow,
};

/// Tools whose call is itself a question for the operator. They never
/// produce a permission event, so their `PreToolUse` is the gate.
pub const QUESTION_TOOLS: [&str; 2] = ["AskUserQuestion", "ExitPlanMode"];

pub fn is_question_tool(name: &str) -> bool {
    QUESTION_TOOLS.contains(&name)
}

/// Adapt one decoded hook frame. Unknown events, and known events missing a
/// field they cannot do without, produce nothing.
pub fn hook(frame: &HookFrame, ctx: &Ctx) -> Vec<Input> {
    let p = &frame.payload;
    let event = |actor: Actor, cause: Cause, kind: EventKind| {
        Input::Event(Event {
            at: Moment {
                local: ctx.now,
                claimed: None,
            },
            process: ctx.process,
            session: frame.session.clone(),
            actor,
            cause,
            confidence: Confidence::Confirmed,
            kind,
        })
    };

    match &*frame.event {
        "SessionStart" => {
            let Some(source) = str_at(p, "source").and_then(|s| s.parse().ok()) else {
                return Vec::new();
            };
            let cause = if source == StartSource::Clear {
                Cause::User
            } else {
                Cause::Agent
            };
            let title = nonblank_str_at(p, "session_title").map(Box::from);
            vec![event(
                actor(p),
                cause,
                EventKind::SessionStarted { source, title },
            )]
        }
        "SessionEnd" => {
            let reason = str_at(p, "reason").unwrap_or("unknown");
            vec![event(
                actor(p),
                Cause::Agent,
                EventKind::SessionEnded {
                    reason: Box::from(reason),
                },
            )]
        }
        "UserPromptSubmit" => {
            let origin = prompt_origin(str_at(p, "source"));
            let cause = if origin == Origin::Human {
                Cause::User
            } else {
                Cause::Agent
            };
            vec![event(
                actor(p),
                cause,
                EventKind::TurnStarted {
                    turn: prompt_key(p),
                    origin,
                },
            )]
        }
        "PreToolUse" => {
            let Some((call, tool)) = call_and_tool(p) else {
                return Vec::new();
            };
            if is_question_tool(&tool) {
                return vec![event(
                    actor(p),
                    Cause::Agent,
                    EventKind::PermissionAsked {
                        call: Some(call),
                        tool,
                        hash: frame_hash(p),
                        owner: Ownership::Human,
                        kind: GateKind::Question,
                    },
                )];
            }
            let Some(hash) = frame_hash(p) else {
                return Vec::new();
            };
            vec![event(
                actor(p),
                Cause::Agent,
                EventKind::ToolDispatched { call, tool, hash },
            )]
        }
        "PermissionRequest" => {
            let Some(tool) = str_at(p, "tool_name").map(ToolName::from) else {
                return Vec::new();
            };
            let kind = if is_question_tool(&tool) {
                GateKind::Question
            } else {
                GateKind::Approval
            };
            vec![event(
                actor(p),
                Cause::Agent,
                EventKind::PermissionAsked {
                    call: None,
                    tool,
                    hash: frame_hash(p),
                    owner: Ownership::Human,
                    kind,
                },
            )]
        }
        "PermissionDenied" => {
            let Some((call, _tool)) = call_and_tool(p) else {
                return Vec::new();
            };
            // `reason` is an open string; only the measured vocabulary maps
            // to a denier, and the event fires on auto-mode denials.
            let by = str_at(p, "reason")
                .and_then(|r| r.parse().ok())
                .unwrap_or(Denier::AutoMode);
            vec![event(
                actor(p),
                Cause::Agent,
                EventKind::PermissionSettled {
                    call: Some(call),
                    req: None,
                    outcome: Decision::Denied { by },
                },
            )]
        }
        "PostToolUse" => {
            let Some((call, _tool)) = call_and_tool(p) else {
                return Vec::new();
            };
            let ms = u64_at(p, "duration_ms").and_then(|ms| u32::try_from(ms).ok());
            vec![event(
                actor(p),
                Cause::Agent,
                EventKind::ToolSettled {
                    call,
                    outcome: ToolOutcome::Ok { ms },
                },
            )]
        }
        "PostToolUseFailure" => {
            let Some((call, _tool)) = call_and_tool(p) else {
                return Vec::new();
            };
            let interrupt = bool_at(p, "is_interrupt").unwrap_or(false);
            let cause = if interrupt { Cause::User } else { Cause::Agent };
            vec![event(
                actor(p),
                cause,
                EventKind::ToolSettled {
                    call,
                    outcome: ToolOutcome::Failed { interrupt },
                },
            )]
        }
        "Stop" => {
            let ended = event(
                actor(p),
                Cause::Agent,
                EventKind::TurnEnded {
                    turn: prompt_key(p),
                    outcome: TurnOutcome::Completed,
                },
            );
            let snapshot = waits(p).map(|rows| {
                Input::Snapshot(Snapshot {
                    taken: ctx.now,
                    kind: SnapshotKind::Waits {
                        session: frame.session.clone(),
                        rows,
                    },
                })
            });
            std::iter::once(ended).chain(snapshot).collect()
        }
        "StopFailure" => {
            let class = ErrorClass::from_wire(str_at(p, "error").unwrap_or(""));
            vec![event(
                actor(p),
                Cause::Agent,
                EventKind::TurnEnded {
                    turn: prompt_key(p),
                    outcome: TurnOutcome::Failed { class },
                },
            )]
        }
        "SubagentStart" => {
            let Some(subagent) = str_at(p, "agent_id").and_then(SubagentId::from_hook) else {
                return Vec::new();
            };
            // Lifecycle events are about the child, but sent by the agent.
            vec![event(
                Actor::Agent,
                Cause::Agent,
                EventKind::SubagentStarted {
                    subagent,
                    kind: subagent_kind(str_at(p, "agent_type")),
                },
            )]
        }
        "SubagentStop" => {
            let Some(subagent) = str_at(p, "agent_id").and_then(SubagentId::from_hook) else {
                return Vec::new();
            };
            vec![event(
                Actor::Agent,
                Cause::Agent,
                EventKind::SubagentEnded {
                    subagent,
                    outcome: SubagentOutcome::Completed,
                },
            )]
        }
        "PreCompact" => {
            let trigger = compaction_trigger(str_at(p, "trigger"));
            let cause = if trigger == Trigger::Manual {
                Cause::User
            } else {
                Cause::Agent
            };
            vec![event(
                actor(p),
                cause,
                EventKind::CompactionStarted { trigger },
            )]
        }
        "PostCompact" => {
            let cause = if compaction_trigger(str_at(p, "trigger")) == Trigger::Manual {
                Cause::User
            } else {
                Cause::Agent
            };
            vec![event(actor(p), cause, EventKind::CompactionEnded)]
        }
        "CwdChanged" => {
            let Some(to) = nonblank_str_at(p, "new_cwd").map(PathBuf::from) else {
                return Vec::new();
            };
            vec![event(actor(p), Cause::Agent, EventKind::CwdChanged { to })]
        }
        "Elicitation" => {
            let tool = nonblank_str_at(p, "mcp_server_name").unwrap_or("Elicitation");
            vec![event(
                actor(p),
                Cause::Agent,
                EventKind::PermissionAsked {
                    call: None,
                    tool: ToolName::from(tool),
                    hash: None,
                    owner: Ownership::Human,
                    kind: GateKind::Question,
                },
            )]
        }
        _ => Vec::new(),
    }
}

/// The hook base carries `agent_id` only when a subagent fired the hook.
fn actor(p: &Value) -> Actor {
    str_at(p, "agent_id")
        .and_then(SubagentId::from_hook)
        .map_or(Actor::Agent, Actor::Subagent)
}

fn prompt_key(p: &Value) -> Option<TurnKey> {
    nonblank_str_at(p, "prompt_id")
        .map(PromptId::from)
        .map(TurnKey::Prompt)
}

/// `UserPromptSubmit.source`. Absent on every captured payload (83 of 83),
/// so absent means the hook cannot say: a machine wake-up and a typed prompt
/// look identical here, and only the transcript's `origin.kind` separates
/// them. Claiming `Human` would make every wake-up user-caused.
fn prompt_origin(source: Option<&str>) -> Origin {
    match source {
        None => Origin::Unknown,
        Some("user") => Origin::Human,
        Some("system") => Origin::TaskNotification,
        Some(_) => Origin::Machine,
    }
}

/// Both fields a tool event cannot do without, or neither.
fn call_and_tool(p: &Value) -> Option<(ToolUseId, ToolName)> {
    nonblank_str_at(p, "tool_use_id")
        .map(ToolUseId::from)
        .zip(str_at(p, "tool_name").map(ToolName::from))
}

/// The forwarder's `tool_input_hash`, or a hash of a raw `tool_input` when a
/// frame arrives unscrubbed.
fn frame_hash(p: &Value) -> Option<InputHash> {
    str_at(p, "tool_input_hash")
        .and_then(InputHash::from_hex)
        .or_else(|| p.get("tool_input").map(input_hash))
}

/// `Stop.background_tasks[]` and `Stop.session_crons[]`, when the payload
/// carries either key. Absent keys mean an older harness or forwarder that
/// cannot report waits, which is not the same as reporting none.
///
/// A task element is `{id, type, status, description}` plus `name` on a
/// workflow and `command` on a shell, as captured. The cron element shape is
/// still inferred: none has ever been captured.
fn waits(p: &Value) -> Option<Vec<WaitRow>> {
    let tasks = p.get("background_tasks");
    let crons = p.get("session_crons");
    if tasks.is_none() && crons.is_none() {
        return None;
    }
    let live_tasks = array_at(p, "background_tasks")
        .filter(|t| !matches!(str_at(t, "status"), Some("completed" | "failed" | "killed")))
        .filter_map(|t| {
            str_at(t, "id").map(|id| WaitRow {
                id: WaitId(Box::from(id)),
                kind: wait_kind(str_at(t, "type")),
            })
        });
    let live_crons = array_at(p, "session_crons").filter_map(|c| {
        str_at(c, "id").map(|id| WaitRow {
            id: WaitId(Box::from(id)),
            kind: WaitKind::Cron,
        })
    });
    Some(live_tasks.chain(live_crons).collect())
}

/// `background_tasks[].type`. The captured vocabulary is `shell`, `monitor`
/// and `workflow`; anything else is carried as `Other` rather than guessed
/// at, so a new task type still counts as a wait.
fn wait_kind(ty: Option<&str>) -> WaitKind {
    match ty {
        Some("shell") => WaitKind::BackgroundShell,
        Some("monitor") => WaitKind::Monitor,
        Some("workflow") => WaitKind::Workflow,
        _ => WaitKind::Other,
    }
}

/// `agent_type`, when it says what kind of child this is. Every captured
/// value is `general-purpose`, a task; nothing documents the teammate or
/// workflow spellings, so those are guesses that fall back to `Task`.
fn subagent_kind(ty: Option<&str>) -> SubagentKind {
    match ty {
        Some("teammate") => SubagentKind::Teammate,
        Some("workflow" | "workflow-agent") => SubagentKind::WorkflowAgent,
        _ => SubagentKind::Task,
    }
}

/// `PreCompact.trigger`: `manual` or `auto`. Absent is not a manual request.
fn compaction_trigger(trigger: Option<&str>) -> Trigger {
    match trigger {
        Some("manual") => Trigger::Manual,
        _ => Trigger::Auto,
    }
}
