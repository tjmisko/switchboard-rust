//! Codex hook frames -> inputs.
//!
//! The mapping, from the captured payloads of September 2026 (13,827
//! `PreToolUse`, 13,459 `PostToolUse`, 183 prompts, 136 stops, 31 permission
//! requests, 25 interrupts, 146 subagent events):
//!
//! | hook                     | input                                   | cause            |
//! |--------------------------|-----------------------------------------|------------------|
//! | SessionStart{source}     | SessionStarted                          | User if `clear`  |
//! | UserPromptSubmit{turn_id}| TurnStarted{Turn(turn_id), Unknown}     | Agent            |
//! | PreToolUse               | ToolDispatched, or PermissionAsked{Question} for `request_user_input` | Agent |
//! | PermissionRequest        | PermissionAsked{call: None, owner: Unknown} | Agent         |
//! | PostToolUse              | ToolSettled{Ok}                         | Agent            |
//! | Stop{turn_id}            | TurnEnded{Completed}                    | Agent            |
//! | Interrupt{turn_id}       | TurnEnded{Interrupted}                  | User             |
//! | SubagentStart / Stop     | SubagentStarted / Ended                 | Agent            |
//! | PreCompact / PostCompact | CompactionStarted / Ended               | User if `manual` |
//! | SessionEnd{reason}       | SessionEnded                            | Agent            |
//!
//! What Codex does not have, and the model already allows for:
//!
//! - **No `tool_use_id` on `PermissionRequest`** (0 of 31 captured), and its
//!   `tool_input` carries the command plus a `description` the call lacks.
//!   The join hashes the command alone, on both sides (`join_input`), and
//!   `PreToolUse` arrives 13-60 ms *before* the request, so the prompt joins
//!   a running call. A later `PreToolUse` never means "approved".
//! - **No owner on a gate.** Codex reviews some requests automatically and
//!   the hook does not say which, so the gate is `Ownership::Unknown` and
//!   renders grey, never red (R3). Only `request_user_input` is certainly
//!   for a person: it arrives on `PreToolUse` with its id, as a question.
//! - **No `PostToolUseFailure`, `PermissionDenied`, `StopFailure`,
//!   `CwdChanged`, status line or waits.** A failed command is a plain
//!   `PostToolUse`; a denied gate is released by `Interrupt`, `Stop` or the
//!   next prompt; limits and turn failures need the app-server.
//! - **`Interrupt` exists**, root only: the exact user-caused release edge
//!   Claude only has in its transcript.
//! - **Turns are keyed by `turn_id`**, which every turn-scoped hook carries.
//! - **`SubagentStop` fires per child turn**, not once per child (up to 64
//!   stops for one child, 3 starts). A row therefore lives from a start to
//!   the child's first stop; later child hooks find no row and are dropped,
//!   except an ask, which parks under the child until it stops or expires.
//!
//! Every hook is `Confidence::Confirmed`, and the actor is the subagent
//! `agent_id` names when present, exactly as for Claude.

use std::path::PathBuf;

use serde_json::Value;

use crate::ingest::json::{nonblank_str_at, str_at};
use crate::ingest::{Ctx, Input, frame::HookFrame, hash::input_hash};
use crate::model::{
    Actor, Cause, Confidence, Event, EventKind, GateKind, InputHash, Moment, Origin, Ownership,
    StartSource, SubagentId, SubagentKind, SubagentOutcome, ToolName, ToolOutcome, ToolUseId,
    Trigger, TurnKey, TurnOutcome,
};

/// Tools whose call is itself a question for the operator. Codex reports
/// them on `PreToolUse` with their id; no permission hook follows.
pub const QUESTION_TOOLS: [&str; 1] = ["request_user_input"];

pub fn is_question_tool(name: &str) -> bool {
    QUESTION_TOOLS.contains(&name)
}

/// The part of a Codex `tool_input` that a prompt and its call share: the
/// command alone. A request adds a `description` the call never has, so a
/// whole-input hash can never match. Inputs without a command (MCP tools,
/// patches) hash whole.
pub fn join_input(input: &Value) -> &Value {
    input.get("command").unwrap_or(input)
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
            vec![event(
                cause,
                EventKind::SessionStarted {
                    source,
                    title: None,
                },
            )]
        }

        // No source field exists to say who typed it, and the rollout file
        // is not read yet, so the origin stays unknown.
        "UserPromptSubmit" => vec![event(
            Cause::Agent,
            EventKind::TurnStarted {
                turn: turn_key(p),
                origin: Origin::Unknown,
            },
        )],

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
                    call: None,
                    tool,
                    hash: frame_hash(p),
                    owner: Ownership::Unknown,
                    kind,
                },
            )]
        }

        // No duration is reported, and a failed command arrives here too.
        "PostToolUse" => {
            let Some(call) = str_at(p, "tool_use_id").map(ToolUseId::from) else {
                return Vec::new();
            };
            vec![event(
                Cause::Agent,
                EventKind::ToolSettled {
                    call,
                    outcome: ToolOutcome::Ok { ms: None },
                },
            )]
        }

        "Stop" => vec![event(
            Cause::Agent,
            EventKind::TurnEnded {
                turn: turn_key(p),
                outcome: TurnOutcome::Completed,
            },
        )],

        "Interrupt" => vec![event(
            Cause::User,
            EventKind::TurnEnded {
                turn: turn_key(p),
                outcome: TurnOutcome::Interrupted { killed: None },
            },
        )],

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
            // These fire in the parent and name the child in `agent_id`.
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

        "SessionEnd" => {
            let reason = str_at(p, "reason").unwrap_or("other").into();
            vec![event(Cause::Agent, EventKind::SessionEnded { reason })]
        }

        // `CwdChanged` never fires on Codex; its cwd stays whatever
        // discovery last saw.
        _ => Vec::new(),
    }
}

/// The hook base carries `agent_id` only when a subagent fired the hook.
/// Not guaranteed on child hooks: `PermissionRequest` never carries it.
fn actor(p: &Value) -> Actor {
    str_at(p, "agent_id")
        .and_then(SubagentId::from_hook)
        .map_or(Actor::Agent, Actor::Subagent)
}

fn turn_key(p: &Value) -> Option<TurnKey> {
    nonblank_str_at(p, "turn_id").map(|id| TurnKey::Turn(id.into()))
}

/// Both fields a tool event cannot do without, or neither.
fn call_and_tool(p: &Value) -> Option<(ToolUseId, ToolName)> {
    let call = str_at(p, "tool_use_id").map(ToolUseId::from);
    let tool = str_at(p, "tool_name").map(ToolName::from);
    call.zip(tool)
}

/// The forwarder's `tool_input_hash`, already over the join subset, or that
/// subset of a raw `tool_input` when a frame arrives unscrubbed.
fn frame_hash(p: &Value) -> Option<InputHash> {
    str_at(p, "tool_input_hash")
        .and_then(InputHash::from_hex)
        .or_else(|| p.get("tool_input").map(|i| input_hash(join_input(i))))
}

/// Unused for now: kept so the cwd column reads the same for both providers
/// once discovery reports it.
#[allow(dead_code)]
fn cwd(p: &Value) -> Option<PathBuf> {
    str_at(p, "cwd").map(PathBuf::from)
}
