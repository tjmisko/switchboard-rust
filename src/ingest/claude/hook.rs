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
//! The one exception is `SubagentStart` and `SubagentStop`. Those fire in
//! the parent, and their `agent_id` names the child being started or
//! stopped, not the sender. Their actor is therefore `Actor::Agent`, and the
//! child goes in the event's `subagent` field.
//!
//! Where a payload leaves something out, the adapter fills in rather than
//! dropping the event, except for the two fields a tool event cannot do
//! without (`tool_use_id`, `tool_name`) and the `agent_id` a lifecycle event
//! is about. The fallbacks are listed on the T03 sheet under "decisions the
//! tests do not make", and each one is pinned by a test in `t03_hook`.
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

/// `UserPromptSubmit.source`. Absent on every captured payload (83 of 83),
/// so absent means the hook cannot say: a machine wake-up and a typed prompt
/// look identical here, and only the transcript's `origin.kind` separates
/// them. Claiming `Human` would make every wake-up user-caused.
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
/// A task element is `{id, type, status, description}` plus `name` on a
/// workflow and `command` on a shell, as captured. The cron element shape is
/// still inferred: none has ever been captured.
fn waits(p: &Value) -> Option<Vec<WaitRow>> {
    todo!("T03")
}

/// `background_tasks[].type`. The captured vocabulary is `shell`, `monitor`
/// and `workflow`; anything else is carried as `Other` rather than guessed
/// at, so a new task type still counts as a wait.
fn wait_kind(ty: Option<&str>) -> WaitKind {
    todo!("T03")
}
