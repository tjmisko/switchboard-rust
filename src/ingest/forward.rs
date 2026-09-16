//! The forwarder's side of the protocol: raw hook stdin in, a scrubbed frame
//! out, and a verdict read back.
//!
//! Content never crosses the socket. Prompts, tool inputs, tool responses and
//! assistant text are dropped here; `tool_input` survives only as its hash.

use serde_json::{Map, Value};

use crate::error::ForwardError;
use crate::ingest::hash::input_hash;

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

/// Turn a raw hook payload into a socket frame.
///
/// Takes the payload by value: the object is edited in place and moved into
/// the result, so the tool input is hashed without being copied.
///
/// `event` comes from argv, which is how the hook is registered; the payload's
/// own `hook_event_name` is the fallback. A payload that is not an object
/// still yields a frame, with nothing in it but the command and event.
pub fn scrub(raw: Value, argv_event: Option<&str>, pid: u32) -> Value {
    let mut map = match raw {
        Value::Object(map) => map,
        _ => Map::new(),
    };

    let event = argv_event
        .map(String::from)
        .or_else(|| map.get("hook_event_name").and_then(Value::as_str).map(String::from));

    if let Some(input) = map.remove("tool_input") {
        map.insert(
            "tool_input_hash".into(),
            Value::String(input_hash(&input).to_hex()),
        );
    }
    for field in CONTENT_FIELDS {
        map.remove(field);
    }

    map.insert("cmd".into(), "hook".into());
    if let Some(event) = event {
        map.insert("event".into(), event.into());
    }
    map.insert("pid".into(), pid.into());
    Value::Object(map)
}

/// Classify the daemon's one-line reply.
pub fn interpret_reply(line: &str) -> Result<(), ForwardError> {
    let reply: Value = serde_json::from_str(line)?;
    match reply.get("ok").and_then(Value::as_bool) {
        Some(true) => Ok(()),
        Some(false) => {
            let reason = reply
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("daemon gave no reason");
            Err(ForwardError::Rejected(Box::from(reason)))
        }
        None => Err(ForwardError::NoVerdict),
    }
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
    }
}
