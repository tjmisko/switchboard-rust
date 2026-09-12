//! Claude transcript lines -> inputs.
//!
//! The transcript is the only source for several user-caused edges that have
//! no hook — Escape during generation above all — so this adapter carries
//! more of the cost model than its size suggests.
//!
//! | line                                         | input                          | cause |
//! |----------------------------------------------|--------------------------------|-------|
//! | user, `interruptedMessageId`                 | TurnEnded{Interrupted{killed}} | User  |
//! | user, `toolDenialKind`, tool_result blocks   | ToolSettled{Denied{by}} each   | User iff `user-rejected` |
//! | user, tool_result blocks                     | ToolSettled{Ok or Failed} each | Agent |
//! | user, string content, not a local command    | TurnStarted{origin}            | User iff human |
//! | assistant, `isApiErrorMessage`               | TurnEnded{Failed{class}} (+ Limits snapshot from `quotaLimits`) | Agent |
//! | assistant, tool_use blocks                   | ToolDispatched each            | Agent |
//! | assistant, `stop_reason: end_turn`           | TurnEnded{Completed}           | Agent |
//!
//! Everything else — metadata lines, attachments, `system` lines, thinking
//! and text blocks — produces nothing.

use std::time::Instant;

use serde_json::Value;

use crate::ingest::json::{array_at, bool_at, str_at, str_path, u64_at};
use crate::ingest::{Input, hash::input_hash};
use crate::model::{
    AccountId, Actor, Cause, Confidence, Denier, ErrorClass, Event, EventKind, LimitStatus,
    LimitWindow, Limits, MessageId, Moment, Origin, ProcKey, SessionId, Snapshot, SnapshotKind,
    ToolName, ToolOutcome, ToolUseId, TurnKey, TurnOutcome,
};
use crate::time::{from_epoch_secs, parse_rfc3339};

/// Everything a transcript line does not say about itself.
#[derive(Debug, Clone)]
pub struct LineCtx {
    pub process: ProcKey,
    /// The session the watcher was started for. Lines carry their own
    /// `sessionId`, but a resumed session's lines disagree with each other.
    pub session: SessionId,
    pub account: AccountId,
    pub now: Instant,
    /// `Inferred` while replaying the tail on discovery, `Confirmed` after.
    pub confidence: Confidence,
}

/// Prefixes of string content that is a local slash command or its output,
/// not a prompt to the agent.
const LOCAL_COMMAND_PREFIXES: [&str; 4] = [
    "<local-command",
    "<command-",
    "<bash-",
    "<user-memory-input",
];

/// Adapt one parsed transcript line.
pub fn line(v: &Value, ctx: &LineCtx) -> Vec<Input> {
    todo!("T05")
}

fn user_line(v: &Value, event: impl Fn(Cause, EventKind) -> Input) -> Vec<Input> {
    todo!("T05")
}

fn assistant_line(
    v: &Value,
    ctx: &LineCtx,
    event: impl Fn(Cause, EventKind) -> Input,
) -> Vec<Input> {
    todo!("T05")
}

/// `quotaLimits` on an API error line: a limits snapshot the transcript
/// happens to carry. Only a rejection with a reset time is worth anything.
fn quota_limits(v: &Value, ctx: &LineCtx) -> Option<Input> {
    todo!("T05")
}

/// `rateLimitType`. The per-model seven-day windows fold into `SevenDay`.
pub fn limit_window(s: &str) -> Option<LimitWindow> {
    todo!("T05")
}
