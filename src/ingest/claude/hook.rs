//! Claude hook frames -> inputs.
//!
//! The mapping, from the Ingestion tab of the signals guide:
//!
//! | hook                     | input                                   | cause            |
//! |--------------------------|-----------------------------------------|------------------|
//! | SessionStart{source}     | SessionStarted                          | User if `clear`  |
//! | UserPromptSubmit         | TurnStarted{prompt_id}                  | User unless machine source |
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

use std::path::PathBuf;

use serde_json::Value;

use crate::ingest::json::{array_at, bool_at, nonblank_str_at, str_at, u64_at};
use crate::ingest::{Ctx, Input, frame::HookFrame, hash::input_hash};
use crate::model::{
    Actor, Cause, Confidence, Decision, Denier, ErrorClass, Event, EventKind, GateKind, InputHash,
    Moment, Origin, Ownership, Snapshot, SnapshotKind, StartSource, SubagentId, SubagentKind,
    SubagentOutcome, ToolName, ToolOutcome, ToolUseId, Trigger, TurnKey, TurnOutcome, WaitId,
    WaitKind, WaitRow,
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
    let event_as = |actor: Actor, cause: Cause, kind: EventKind| {
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
    let event = |cause: Cause, kind: EventKind| event_as(actor(p), cause, kind);

    match &*frame.event {
        "SessionStart" => {
            let source = str_at(p, "source")
                .and_then(|s| s.parse::<StartSource>().ok())
                .unwrap_or(StartSource::Startup);
            let cause = if source == StartSource::Clear {
                Cause::User
            } else {
                Cause::Agent
            };
            let title = nonblank_str_at(p, "session_title").map(Box::from);
            vec![event(cause, EventKind::SessionStarted { source, title })]
        }

        "UserPromptSubmit" => {
            let origin = prompt_origin(str_at(p, "source"));
            let cause = if origin == Origin::Human {
                Cause::User
            } else {
                Cause::Agent
            };
            vec![event(
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
            let kind = if is_question_tool(&tool) {
                EventKind::PermissionAsked {
                    call: Some(call),
                    tool,
                    hash: frame_hash(p),
                    owner: Ownership::Human,
                    kind: GateKind::Question,
                }
            } else {
                EventKind::ToolDispatched {
                    call,
                    tool,
                    hash: frame_hash(p).unwrap_or_else(|| input_hash(&Value::Null)),
                }
            };
            vec![event(Cause::Agent, kind)]
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
                Cause::Agent,
                EventKind::PermissionAsked {
                    call: str_at(p, "tool_use_id").map(ToolUseId::from),
                    tool,
                    hash: frame_hash(p),
                    owner: Ownership::Human,
                    kind,
                },
            )]
        }

        "PermissionDenied" => vec![event(
            Cause::Agent,
            EventKind::PermissionSettled {
                call: str_at(p, "tool_use_id").map(ToolUseId::from),
                req: None,
                // `reason` is an open string and whether a human decline
                // fires this hook is unmeasured. The transcript's
                // `toolDenialKind` stays authoritative for who denied.
                outcome: Decision::Denied {
                    by: Denier::PermissionRule,
                },
            },
        )],

        "PostToolUse" => {
            let Some(call) = str_at(p, "tool_use_id").map(ToolUseId::from) else {
                return Vec::new();
            };
            let ms = u64_at(p, "duration_ms").and_then(|ms| u32::try_from(ms).ok());
            vec![event(
                Cause::Agent,
                EventKind::ToolSettled {
                    call,
                    outcome: ToolOutcome::Ok { ms },
                },
            )]
        }

        "PostToolUseFailure" => {
            let Some(call) = str_at(p, "tool_use_id").map(ToolUseId::from) else {
                return Vec::new();
            };
            let interrupt = bool_at(p, "is_interrupt").unwrap_or(false);
            let cause = if interrupt { Cause::User } else { Cause::Agent };
            vec![event(
                cause,
                EventKind::ToolSettled {
                    call,
                    outcome: ToolOutcome::Failed { interrupt },
                },
            )]
        }

        "Stop" => {
            let mut out = vec![event(
                Cause::Agent,
                EventKind::TurnEnded {
                    turn: prompt_key(p),
                    outcome: TurnOutcome::Completed,
                },
            )];
            if let Some(rows) = waits(p) {
                out.push(Input::Snapshot(Snapshot {
                    taken: ctx.now,
                    kind: SnapshotKind::Waits {
                        session: frame.session.clone(),
                        rows,
                    },
                }));
            }
            out
        }

        "StopFailure" => {
            let class = str_at(p, "error")
                .map(ErrorClass::from_wire)
                .unwrap_or(ErrorClass::Other);
            vec![event(
                Cause::Agent,
                EventKind::TurnEnded {
                    turn: prompt_key(p),
                    outcome: TurnOutcome::Failed { class },
                },
            )]
        }

        "SubagentStart" | "SubagentStop" => {
            let Some(subagent) = str_at(p, "agent_id").and_then(SubagentId::from_hook) else {
                return Vec::new();
            };
            let kind = if &*frame.event == "SubagentStart" {
                EventKind::SubagentStarted {
                    subagent,
                    kind: SubagentKind::Task,
                }
            } else {
                EventKind::SubagentEnded {
                    subagent,
                    outcome: SubagentOutcome::Completed,
                }
            };
            // These fire in the parent and name the child in `agent_id`, so
            // the actor is the agent, not the subagent `actor()` would read.
            vec![event_as(Actor::Agent, Cause::Agent, kind)]
        }

        "PreCompact" => {
            let trigger = match str_at(p, "trigger") {
                Some("manual") => Trigger::Manual,
                _ => Trigger::Auto,
            };
            let cause = if trigger == Trigger::Manual {
                Cause::User
            } else {
                Cause::Agent
            };
            vec![event(cause, EventKind::CompactionStarted { trigger })]
        }

        "PostCompact" => vec![event(Cause::Agent, EventKind::CompactionEnded)],

        "CwdChanged" => str_at(p, "new_cwd")
            .map(|to| {
                event(
                    Cause::Agent,
                    EventKind::CwdChanged {
                        to: PathBuf::from(to),
                    },
                )
            })
            .into_iter()
            .collect(),

        "SessionEnd" => {
            let reason = str_at(p, "reason").unwrap_or("other").into();
            vec![event(Cause::Agent, EventKind::SessionEnded { reason })]
        }

        "Elicitation" => {
            let tool = str_at(p, "mcp_server_name").unwrap_or("mcp");
            vec![event(
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
    nonblank_str_at(p, "prompt_id").map(|id| TurnKey::Prompt(id.into()))
}

/// `UserPromptSubmit.source`. Absent means an older harness, whose prompts
/// were all typed.
fn prompt_origin(source: Option<&str>) -> Origin {
    match source {
        None | Some("user") => Origin::Human,
        Some("system") => Origin::TaskNotification,
        Some(_) => Origin::Machine,
    }
}

/// Both fields a tool event cannot do without, or neither.
fn call_and_tool(p: &Value) -> Option<(ToolUseId, ToolName)> {
    let call = str_at(p, "tool_use_id").map(ToolUseId::from);
    let tool = str_at(p, "tool_name").map(ToolName::from);
    call.zip(tool)
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
/// Element shapes are inferred from the bundle, not recorded.
fn waits(p: &Value) -> Option<Vec<WaitRow>> {
    if p.get("background_tasks").is_none() && p.get("session_crons").is_none() {
        return None;
    }
    let tasks = array_at(p, "background_tasks")
        .filter(|t| matches!(str_at(t, "status"), None | Some("running" | "pending")))
        .filter_map(|t| {
            let id = str_at(t, "id")?;
            let kind = match str_at(t, "type") {
                Some(ty) if ty.contains("monitor") => WaitKind::Monitor,
                _ => WaitKind::BackgroundShell,
            };
            Some(WaitRow {
                id: WaitId(id.into()),
                kind,
            })
        });
    let crons = array_at(p, "session_crons").filter_map(|c| {
        str_at(c, "id").map(|id| WaitRow {
            id: WaitId(id.into()),
            kind: WaitKind::Cron,
        })
    });
    Some(tasks.chain(crons).collect())
}
