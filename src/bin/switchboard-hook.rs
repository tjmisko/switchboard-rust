//! `switchboard-hook <Event>`: the Claude hook forwarder.
//!
//! Registered as a hook command. Reads the payload on stdin, scrubs it, sends
//! one frame to the daemon, and exits 0 no matter what: a broken tracker must
//! never break the agent it tracks.
//!
//! `switchboard-hook statusline` forwards a status-line payload instead.
//! Errors are silent unless `SWITCHBOARD_DEBUG` is set.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use switchboard::client;
use switchboard::ingest::forward::scrub;
use switchboard::paths::socket_path;

/// The hook budget is 2 s. Leave most of it to the agent.
const TIMEOUT: Duration = Duration::from_millis(500);
/// Payloads are read whole; anything larger is not a hook payload.
const MAX_STDIN: u64 = 4 << 20;

fn main() {
    let arg = std::env::args().nth(1);

    let mut raw = Vec::new();
    let _ = std::io::stdin().take(MAX_STDIN).read_to_end(&mut raw);
    let payload: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    let pid = std::os::unix::process::parent_id();

    let frame = match arg.as_deref() {
        Some("statusline") => statusline_frame(payload, pid),
        event => scrub(payload, event, pid),
    };

    if let Err(e) = client::send(&socket_path(), &frame, TIMEOUT)
        && std::env::var_os("SWITCHBOARD_DEBUG").is_some()
    {
        eprintln!("switchboard-hook: {e}");
    }
}

fn statusline_frame(payload: Value, pid: u32) -> Value {
    let mut frame = match payload {
        Value::Object(map) => Value::Object(map),
        _ => serde_json::json!({}),
    };
    frame["cmd"] = "statusline".into();
    frame["pid"] = pid.into();
    frame
}
