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
    if bool_at(v, "isSidechain") == Some(true) || bool_at(v, "isMeta") == Some(true) {
        return Vec::new();
    }
    let at = Moment {
        local: ctx.now,
        claimed: str_at(v, "timestamp").and_then(parse_rfc3339),
    };
    let event = |cause: Cause, kind: EventKind| {
        Input::Event(Event {
            at,
            process: Some(ctx.process),
            session: ctx.session.clone(),
            actor: Actor::Agent,
            cause,
            confidence: ctx.confidence,
            kind,
        })
    };

    match str_at(v, "type") {
        Some("user") => user_line(v, event),
        Some("assistant") => assistant_line(v, ctx, event),
        _ => Vec::new(),
    }
}

fn user_line(v: &Value, event: impl Fn(Cause, EventKind) -> Input) -> Vec<Input> {
    let turn = str_at(v, "promptId").map(|id| TurnKey::Prompt(id.into()));

    // Key on the field, never on the marker text: two marker strings exist.
    if let Some(killed) = str_at(v, "interruptedMessageId") {
        return vec![event(
            Cause::User,
            EventKind::TurnEnded {
                turn,
                outcome: TurnOutcome::Interrupted {
                    killed: Some(MessageId::from(killed)),
                },
            },
        )];
    }

    let content = v.get("message").and_then(|m| m.get("content"));

    // A typed prompt is a bare string; a tool result is an array of blocks.
    if let Some(text) = content.and_then(Value::as_str) {
        if LOCAL_COMMAND_PREFIXES.iter().any(|p| text.starts_with(p)) {
            return Vec::new();
        }
        let origin = str_path(v, &["origin", "kind"]).map_or(Origin::Human, Origin::from_kind);
        let cause = if origin == Origin::Human {
            Cause::User
        } else {
            Cause::Agent
        };
        return vec![event(cause, EventKind::TurnStarted { turn, origin })];
    }

    let denial =
        str_at(v, "toolDenialKind").map(|k| k.parse::<Denier>().unwrap_or(Denier::PermissionRule));

    let Some(message) = v.get("message") else {
        return Vec::new();
    };
    array_at(message, "content")
        .filter(|block| str_at(block, "type") == Some("tool_result"))
        .filter_map(|block| {
            let call = ToolUseId::from(str_at(block, "tool_use_id")?);
            let failed = bool_at(block, "is_error").unwrap_or(false);
            let (cause, outcome) = match denial {
                Some(Denier::User) => (Cause::User, ToolOutcome::Denied { by: Denier::User }),
                Some(by) => (Cause::Agent, ToolOutcome::Denied { by }),
                None if failed => (Cause::Agent, ToolOutcome::Failed { interrupt: false }),
                None => (Cause::Agent, ToolOutcome::Ok { ms: None }),
            };
            Some(event(cause, EventKind::ToolSettled { call, outcome }))
        })
        .collect()
}

fn assistant_line(
    v: &Value,
    ctx: &LineCtx,
    event: impl Fn(Cause, EventKind) -> Input,
) -> Vec<Input> {
    if bool_at(v, "isApiErrorMessage") == Some(true) {
        let class = str_at(v, "error")
            .map(ErrorClass::from_wire)
            .unwrap_or(ErrorClass::Other);
        let mut out = vec![event(
            Cause::Agent,
            EventKind::TurnEnded {
                turn: None,
                outcome: TurnOutcome::Failed { class },
            },
        )];
        out.extend(quota_limits(v, ctx));
        return out;
    }

    let Some(message) = v.get("message") else {
        return Vec::new();
    };

    let dispatched = array_at(message, "content")
        .filter(|block| str_at(block, "type") == Some("tool_use"))
        .filter_map(|block| {
            let call = ToolUseId::from(str_at(block, "id")?);
            let tool = ToolName::from(str_at(block, "name")?);
            let hash = input_hash(block.get("input").unwrap_or(&Value::Null));
            Some(event(
                Cause::Agent,
                EventKind::ToolDispatched { call, tool, hash },
            ))
        });

    let ended = (str_at(message, "stop_reason") == Some("end_turn")).then(|| {
        event(
            Cause::Agent,
            EventKind::TurnEnded {
                turn: None,
                outcome: TurnOutcome::Completed,
            },
        )
    });

    dispatched.chain(ended).collect()
}

/// `quotaLimits` on an API error line: a limits snapshot the transcript
/// happens to carry. Only a rejection with a reset time is worth anything.
fn quota_limits(v: &Value, ctx: &LineCtx) -> Option<Input> {
    let q = v.get("quotaLimits")?;
    let status = match str_at(q, "status")? {
        "rejected" => LimitStatus::Rejected,
        "allowed_warning" => LimitStatus::Warning,
        "allowed" => LimitStatus::Allowed,
        _ => return None,
    };
    let resets_at = u64_at(q, "resetsAt").and_then(from_epoch_secs);
    Some(Input::Snapshot(Snapshot {
        taken: ctx.now,
        kind: SnapshotKind::Limits {
            account: ctx.account.clone(),
            value: Limits {
                status,
                resets_at,
                window: str_at(q, "rateLimitType").and_then(limit_window),
                used_percent: None,
            },
        },
    }))
}

/// `rateLimitType`. The per-model seven-day windows fold into `SevenDay`.
pub fn limit_window(s: &str) -> Option<LimitWindow> {
    match s {
        "five_hour" => Some(LimitWindow::FiveHour),
        "overage" | "seven_day_overage_included" => Some(LimitWindow::Overage),
        s if s.starts_with("seven_day") => Some(LimitWindow::SevenDay),
        _ => None,
    }
}
