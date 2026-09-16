//! Claude transcript lines -> inputs.
//!
//! The transcript is the only source for several user-caused edges that have
//! no hook — Escape during generation above all — so this adapter carries
//! more of the cost model than its size suggests.
//!
//! | line                                         | input                          | cause |
//! |----------------------------------------------|--------------------------------|-------|
//! | any type, `isSidechain: true`                | nothing                        |       |
//! | any type, `isMeta: true`                     | nothing                        |       |
//! | user, `interruptedMessageId`                 | TurnEnded{Interrupted{killed}}, `turn` from `promptId` | User  |
//! | user, `toolDenialKind`, tool_result blocks   | ToolSettled{Denied{by}} each   | User iff `user-rejected` |
//! | user, tool_result blocks                     | ToolSettled{Ok or Failed} each | Agent |
//! | user, string content, not a local command    | TurnStarted{origin}, `turn` from `promptId` | User iff `origin.kind` is `human` |
//! | user, string content, no `origin`            | TurnStarted{Human}             | User  |
//! | assistant, `isApiErrorMessage`               | TurnEnded{Failed{class}}, then a Limits snapshot from `quotaLimits` | Agent |
//! | assistant, tool_use blocks                   | ToolDispatched each            | Agent |
//! | assistant, `stop_reason: end_turn`           | TurnEnded{Completed}           | Agent |
//!
//! The first two rows are checked before the line's type. A sidechain line
//! is a subagent's conversation inlined into the parent's file, and an
//! `isMeta` line is text the client injected (a caveat after a `!` command, a
//! system reminder, a teammate message). Neither is this session speaking.
//!
//! A prompt line with no `origin` at all is read as a human prompt. That is
//! the opposite of the hook side, where `UserPromptSubmit` never carries
//! `source` and an absent one means nothing (`Origin::Unknown`). Here absence
//! is the common case for typed prompts: 35 of 53 human prompt lines in this
//! machine's transcripts carry no `origin` (docs/product/13, F7), while every
//! machine-started prompt names its `origin.kind`. So absent means human, and
//! a named kind is what says otherwise.
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
/// `used_percent` stays `None`: the transcript never reports one.
fn quota_limits(v: &Value, ctx: &LineCtx) -> Option<Input> {
    todo!("T05")
}

/// `rateLimitType`, as the harness names its windows. The vocabulary is
/// `five_hour`, `seven_day`, `seven_day_opus`, `seven_day_sonnet`,
/// `seven_day_overage_included` and `overage` (`statusline::WINDOWS` lists
/// the per-window keys; survey 13 §2.3, "Limit names", has the full set with
/// the text each one renders as). It folds to the three windows the model
/// keeps:
///
/// - `five_hour` → `FiveHour`
/// - `overage` and `seven_day_overage_included` → `Overage`
/// - anything else starting with `seven_day` → `SevenDay`
/// - anything else → `None`
pub fn limit_window(s: &str) -> Option<LimitWindow> {
    todo!("T05")
}
