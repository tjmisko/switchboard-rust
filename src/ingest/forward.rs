//! The forwarder's side of the protocol: raw hook stdin in, a scrubbed frame
//! out, and a verdict read back.
//!
//! Content never crosses the socket. Prompts, tool inputs, tool responses and
//! assistant text are dropped here; `tool_input` survives only as its hash.

use serde_json::{Map, Value};

use crate::error::ForwardError;
use crate::ingest::codex;
use crate::ingest::hash::input_hash;
use crate::model::Provider;

/// Fields removed before a frame leaves the forwarder.
pub const CONTENT_FIELDS: [&str; 7] = [
    "tool_input",
    "tool_response",
    "prompt",
    "last_assistant_message",
    "message",
    "custom_instructions",
    "requested_schema",
];

/// Turn a raw Claude hook payload into a socket frame.
///
/// Takes the payload by value: the object is edited in place and moved into
/// the result, so the tool input is hashed without being copied.
///
/// `event` comes from argv, which is how the hook is registered; the payload's
/// own `hook_event_name` is the fallback. A payload that is not an object
/// still yields a frame, with nothing in it but the command and event.
pub fn scrub(raw: Value, argv_event: Option<&str>, pid: u32) -> Value {
    scrub_for(Provider::Claude, raw, argv_event, pid)
}

/// `scrub`, for whichever harness the hook is registered under.
///
/// The hash a permission prompt joins on is provider-specific: Codex's
/// `PermissionRequest` carries the command plus a `description` the call
/// never has, so its subset is the command alone (`codex::join_input`), and
/// the frame names its provider so the daemon picks the matching adapter.
/// Claude frames stay exactly as before: whole input, no provider field.
pub fn scrub_for(provider: Provider, raw: Value, argv_event: Option<&str>, pid: u32) -> Value {
    let mut obj = match raw {
        Value::Object(map) => map,
        _ => Map::new(),
    };

    let event = argv_event
        .map(str::to_owned)
        .or_else(|| {
            obj.get("hook_event_name")
                .and_then(Value::as_str)
                .map(str::to_owned)
        })
        .unwrap_or_default();

    if let Some(input) = obj.remove("tool_input") {
        let joined = match provider {
            Provider::Claude => &input,
            Provider::Codex => codex::join_input(&input),
        };
        obj.insert("tool_input_hash".into(), input_hash(joined).to_hex().into());
    }
    for field in CONTENT_FIELDS {
        obj.remove(field);
    }

    obj.insert("cmd".into(), "hook".into());
    obj.insert("event".into(), event.into());
    obj.insert("pid".into(), pid.into());
    if provider != Provider::Claude {
        obj.insert("provider".into(), provider.as_str().into());
    }
    Value::Object(obj)
}

/// Classify the daemon's one-line reply.
pub fn interpret_reply(line: &str) -> Result<(), ForwardError> {
    let v: Value = serde_json::from_str(line)?;
    let ok = v
        .get("ok")
        .and_then(Value::as_bool)
        .ok_or(ForwardError::NoVerdict)?;
    if ok {
        return Ok(());
    }
    let why = v
        .get("error")
        .and_then(Value::as_str)
        .unwrap_or("no reason given");
    Err(ForwardError::Rejected(why.into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn should_replace_tool_input_with_its_hash_when_scrubbing() {
        let raw =
            json!({"session_id": "s", "tool_input": {"command": "rm -rf /"}, "prompt": "secret"});
        let out = scrub(raw, Some("PermissionRequest"), 7);
        assert!(out.get("tool_input").is_none());
        assert!(out.get("prompt").is_none());
        assert_eq!(out["tool_input_hash"].as_str().map(str::len), Some(16));
        assert_eq!(out["event"], "PermissionRequest");
        assert_eq!(out["pid"], 7);
        assert!(
            out.get("provider").is_none(),
            "Claude frames name no provider"
        );
    }

    #[test]
    fn should_hash_only_the_command_and_name_the_provider_for_codex() {
        let call = json!({"session_id": "t", "tool_input": {"command": "cargo test"}});
        let request = json!({"session_id": "t",
            "tool_input": {"command": "cargo test", "description": "run the suite"}});
        let a = scrub_for(Provider::Codex, call, Some("PreToolUse"), 7);
        let b = scrub_for(Provider::Codex, request, Some("PermissionRequest"), 7);
        assert_eq!(a["tool_input_hash"], b["tool_input_hash"]);
        assert_eq!(a["provider"], "codex");
    }
}
