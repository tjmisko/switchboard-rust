//! Identifier newtypes and wire vocabulary, as strings.
//!
//! Everything here is about moving between an owned identifier and the
//! borrowed `str` it wraps without losing the type on the way in:
//!
//!   * `Deref<Target = str>` lets `&SessionId` go anywhere `&str` goes, by
//!     coercion, while a `SessionId` still cannot be passed as a `ToolUseId`.
//!   * `Borrow<str>` lets a `BTreeMap<SessionId, _>` be queried with a plain
//!     `&str`, so a lookup from wire data allocates nothing.
//!   * `From<&str>` and `From<String>` are the only ways in, and the `String`
//!     one reuses the allocation.

use std::borrow::Borrow;
use std::fmt;
use std::ops::Deref;
use std::str::FromStr;

use crate::error::UnknownName;
use crate::model::{
    AccountId, Denier, ErrorClass, Host, MessageId, Origin, Phase, PromptId, RequestId, SessionId,
    StartSource, SubagentId, ToolName, ToolUseId, TurnId,
};

macro_rules! id_impls {
    ($($name:ident),* $(,)?) => {$(
        impl $name {
            pub fn as_str(&self) -> &str {
                todo!("T01")
            }
        }

        impl Deref for $name {
            type Target = str;
            fn deref(&self) -> &str {
                todo!("T01")
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                todo!("T01")
            }
        }

        // What contract does `Borrow` add over `AsRef`, and why does this
        // type meet it? (T01 kata territory.)
        impl Borrow<str> for $name {
            fn borrow(&self) -> &str {
                todo!("T01")
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                todo!("T01")
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                todo!("T01")
            }
        }

        /// Must reuse the `String`'s allocation.
        impl From<String> for $name {
            fn from(s: String) -> Self {
                todo!("T01")
            }
        }
    )*};
}

id_impls!(
    SessionId, PromptId, TurnId, ToolUseId, SubagentId, RequestId, MessageId, AccountId, ToolName,
    Host,
);

impl SubagentId {
    /// The filename stem in `subagents/` is `agent-<id>`. Strip that prefix
    /// exactly once: a subagent genuinely named `agent-x` lives at
    /// `agent-agent-x`, and stripping twice would call it `x`.
    pub fn from_file_stem(stem: &str) -> Option<SubagentId> {
        todo!("T01")
    }

    /// The hook `agent_id` field is already the bare id. Taken as-is, with
    /// surrounding whitespace removed; blank means "not a subagent".
    pub fn from_hook(raw: &str) -> Option<SubagentId> {
        todo!("T01")
    }
}

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

impl Phase {
    pub const ALL: [Phase; 8] = [
        Phase::Attention,
        Phase::Blocked,
        Phase::Stopped,
        Phase::Unknown,
        Phase::Working,
        Phase::Delegating,
        Phase::Waiting,
        Phase::Idle,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Phase::Attention => "attention",
            Phase::Blocked => "blocked",
            Phase::Stopped => "stopped",
            Phase::Unknown => "unknown",
            Phase::Working => "working",
            Phase::Delegating => "delegating",
            Phase::Waiting => "waiting",
            Phase::Idle => "idle",
        }
    }
}

impl fmt::Display for Phase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for Phase {
    type Err = UnknownName;

    fn from_str(s: &str) -> Result<Phase, UnknownName> {
        todo!("T02")
    }
}

impl FromStr for StartSource {
    type Err = UnknownName;

    fn from_str(s: &str) -> Result<StartSource, UnknownName> {
        todo!("T02")
    }
}

impl FromStr for Denier {
    type Err = UnknownName;

    /// The transcript's `toolDenialKind` vocabulary.
    fn from_str(s: &str) -> Result<Denier, UnknownName> {
        todo!("T02")
    }
}

impl Origin {
    /// Transcript `origin.kind`. Total, because a new kind of wake-up must
    /// not make a prompt disappear: anything unrecognised is a machine.
    pub fn from_kind(s: &str) -> Origin {
        match s {
            "human" => Origin::Human,
            "task-notification" => Origin::TaskNotification,
            "peer" => Origin::Peer,
            "coordinator" => Origin::Coordinator,
            "auto-continuation" => Origin::AutoContinuation,
            _ => Origin::Machine,
        }
    }
}

impl ErrorClass {
    /// Transcript `error` on an `isApiErrorMessage` line, or the
    /// `StopFailure.error` hook field. Total: `Other` absorbs new values.
    pub fn from_wire(s: &str) -> ErrorClass {
        match s {
            "rate_limit" => ErrorClass::RateLimit,
            "server_error" | "overloaded" => ErrorClass::ServerError,
            "authentication_failed" | "oauth_org_not_allowed" => ErrorClass::AuthFailed,
            "connection_failed" => ErrorClass::ConnectionFailed,
            "stream_disconnected" => ErrorClass::StreamDisconnected,
            "too_many_retries" => ErrorClass::TooManyRetries,
            _ => ErrorClass::Other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn takes_str(s: &str) -> usize {
        s.len()
    }

    #[test]
    fn should_coerce_an_id_reference_to_str_when_passed_to_a_str_parameter() {
        let id = SessionId::from("sess-1");
        assert_eq!(takes_str(&id), 6);
        assert!(id.starts_with("sess"));
    }

    #[test]
    fn should_look_up_an_id_keyed_map_by_str_when_borrow_is_implemented() {
        let mut m = BTreeMap::new();
        m.insert(SessionId::from("a"), 1);
        assert_eq!(m.get("a"), Some(&1));
    }

    #[test]
    fn should_strip_the_agent_prefix_exactly_once_when_reading_a_file_stem() {
        assert_eq!(
            SubagentId::from_file_stem("agent-agent-x"),
            Some(SubagentId::from("agent-x"))
        );
        assert_eq!(SubagentId::from_file_stem("agent-"), None);
        assert_eq!(SubagentId::from_file_stem("other"), None);
    }

    #[test]
    fn should_round_trip_every_phase_through_its_name() {
        for p in Phase::ALL {
            assert_eq!(p.as_str().parse::<Phase>(), Ok(p));
        }
        assert!("permission".parse::<Phase>().is_err());
    }
}
