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
    todo!("T03")
}

/// The hook base carries `agent_id` only when a subagent fired the hook.
fn actor(p: &Value) -> Actor {
    todo!("T03")
}

fn prompt_key(p: &Value) -> Option<TurnKey> {
    todo!("T03")
}

/// `UserPromptSubmit.source`. Absent means an older harness, whose prompts
/// were all typed.
fn prompt_origin(source: Option<&str>) -> Origin {
    todo!("T03")
}

/// Both fields a tool event cannot do without, or neither.
fn call_and_tool(p: &Value) -> Option<(ToolUseId, ToolName)> {
    todo!("T03")
}

/// The forwarder's `tool_input_hash`, or a hash of a raw `tool_input` when a
/// frame arrives unscrubbed.
fn frame_hash(p: &Value) -> Option<InputHash> {
    todo!("T03")
}

/// `Stop.background_tasks[]` and `Stop.session_crons[]`, when the payload
/// carries either key. Absent keys mean an older harness or forwarder that
/// cannot report waits, which is not the same as reporting none.
///
/// Element shapes are inferred from the bundle, not recorded.
fn waits(p: &Value) -> Option<Vec<WaitRow>> {
    todo!("T03")
}
