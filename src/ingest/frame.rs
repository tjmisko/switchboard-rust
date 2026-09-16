//! The daemon socket protocol: one JSON object per line, each way.
//!
//! ```text
//! -> {"cmd":"hook","event":"PermissionRequest","pid":4036,"session_id":"…", …payload}
//! <- {"ok":true}
//! -> {"cmd":"statusline","pid":4036,"session_id":"…", …payload}
//! <- {"ok":true}
//! -> {"cmd":"list"}
//! <- {"snapshot":{…}}
//! -> {"cmd":"subscribe"}
//! <- {"snapshot":{…}}   once now, then again on every change
//! <- {"ok":false,"error":"…"}   for any line that does not decode
//! ```
//!
//! Decoding is where `Option` becomes `Result`. A field the protocol requires
//! is an error when absent; a field it allows to be absent is an error only
//! when present with the wrong type.

use serde_json::{Value, json};

use crate::error::FrameError;
use crate::model::SessionId;

/// A decoded request line.
#[derive(Debug, Clone, PartialEq)]
pub enum Request {
    Hook(HookFrame),
    Statusline(StatuslineFrame),
    List,
    Subscribe,
}

/// A hook payload as the forwarder sent it: already scrubbed of content, with
/// `tool_input` replaced by `tool_input_hash`.
#[derive(Debug, Clone, PartialEq)]
pub struct HookFrame {
    /// The hook event name, e.g. `PermissionRequest`.
    pub event: Box<str>,
    /// The forwarder's parent pid. Absent when the forwarder could not tell.
    pub pid: Option<u32>,
    pub session: SessionId,
    /// The whole frame, for the adapter to read event-specific fields from.
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StatuslineFrame {
    pub pid: Option<u32>,
    pub session: SessionId,
    pub payload: Value,
}

/// Decode one request line.
pub fn decode_request(line: &str) -> Result<Request, FrameError> {
    let v: Value = serde_json::from_str(line)?;
    if !v.is_object() {
        return Err(FrameError::NotAnObject);
    }
    match required_str(&v, "cmd")? {
        "list" => Ok(Request::List),
        "subscribe" => Ok(Request::Subscribe),
        "hook" => {
            let event = Box::from(required_str(&v, "event")?);
            let session = SessionId::from(required_str(&v, "session_id")?);
            let pid = optional_pid(&v)?;
            Ok(Request::Hook(HookFrame {
                event,
                pid,
                session,
                payload: v,
            }))
        }
        "statusline" => {
            let session = SessionId::from(required_str(&v, "session_id")?);
            let pid = optional_pid(&v)?;
            Ok(Request::Statusline(StatuslineFrame {
                pid,
                session,
                payload: v,
            }))
        }
        other => Err(FrameError::UnknownCommand(Box::from(other))),
    }
}

/// A string field the protocol requires. Absent and mistyped are different
/// errors, because they point at different bugs in the sender.
fn required_str<'a>(v: &'a Value, field: &'static str) -> Result<&'a str, FrameError> {
    let raw = v.get(field).ok_or(FrameError::Missing(field))?;
    raw.as_str().ok_or(FrameError::WrongType {
        field,
        expected: "a string",
    })
}

/// `pid` may be absent. When present it must fit a `u32`.
///
/// `Option<&Value>` -> `Option<Result<u32, _>>` -> `Result<Option<u32>, _>`:
/// `transpose` is what lets `?` reject a bad value without also rejecting a
/// missing one.
fn optional_pid(v: &Value) -> Result<Option<u32>, FrameError> {
    v.get("pid") // Option<&Value>: None when the field is absent
        .map(|raw| {
            // inside: &Value -> Result<u32, FrameError>
            raw.as_u64() // Option<u64>: None for "4036", -1, 1.5
                .and_then(|n| u32::try_from(n).ok()) // Option<u32>: None for 2^32
                .ok_or(FrameError::WrongType {
                    field: "pid",
                    expected: "a u32",
                }) // Result<u32, FrameError>
        }) // Option<Result<u32, FrameError>>
        .transpose() // Result<Option<u32>, FrameError>
}

/// `{"ok":true}`
pub fn ok_reply() -> String {
    json!({"ok": true}).to_string()
}

/// `{"ok":false,"error":"…"}`
pub fn error_reply(e: &FrameError) -> String {
    json!({"ok": false, "error": e.to_string()}).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_decode_list_when_only_cmd_is_given() {
        assert_eq!(decode_request(r#"{"cmd":"list"}"#).unwrap(), Request::List);
    }

    #[test]
    fn should_reject_a_string_pid_but_accept_a_missing_one() {
        let bad = r#"{"cmd":"hook","event":"Stop","pid":"12","session_id":"s"}"#;
        assert!(matches!(
            decode_request(bad),
            Err(FrameError::WrongType { field: "pid", .. })
        ));
        let missing = r#"{"cmd":"hook","event":"Stop","session_id":"s"}"#;
        assert!(matches!(
            decode_request(missing),
            Ok(Request::Hook(HookFrame { pid: None, .. }))
        ));
    }
}
