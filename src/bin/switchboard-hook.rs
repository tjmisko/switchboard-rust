//! `switchboard-hook [claude|codex] <Event>`: the hook forwarder.
//!
//! Registered as a hook command. Reads the payload on stdin, scrubs it, sends
//! one frame to the daemon, and exits 0 no matter what: a broken tracker must
//! never break the agent it tracks.
//!
//! The provider names the harness the hook is registered under, so the
//! forwarder hashes the right join subset and the daemon picks the right
//! adapter. It defaults to Claude, which keeps every existing registration
//! working unchanged.
//!
//! `switchboard-hook statusline` forwards a status-line payload instead.
//! Errors are silent unless `SWITCHBOARD_DEBUG` is set.

use std::io::Read;
use std::time::Duration;

use serde_json::Value;

use switchboard::client;
use switchboard::ingest::forward::scrub_for;
use switchboard::model::Provider;
use switchboard::paths::socket_path;

/// The hook budget is 2 s. Leave most of it to the agent.
const TIMEOUT: Duration = Duration::from_millis(500);
/// Payloads are read whole; anything larger is not a hook payload.
const MAX_STDIN: u64 = 4 << 20;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (provider, arg) = registration(&args);

    let mut raw = Vec::new();
    let _ = std::io::stdin().take(MAX_STDIN).read_to_end(&mut raw);
    let payload: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    let pid = std::os::unix::process::parent_id();

    let frame = match arg {
        Some("statusline") => statusline_frame(payload, pid),
        event => scrub_for(provider, payload, event, pid),
    };

    if let Err(e) = client::send(&socket_path(), &frame, TIMEOUT)
        && std::env::var_os("SWITCHBOARD_DEBUG").is_some()
    {
        eprintln!("switchboard-hook: {e}");
    }
}

/// `[provider] [event]` from argv. A first argument that names a provider is
/// one; anything else is the event, under Claude.
fn registration(args: &[String]) -> (Provider, Option<&str>) {
    match args {
        [first, rest @ ..] => match first.parse::<Provider>() {
            Ok(provider) => (provider, rest.first().map(String::as_str)),
            Err(_) => (Provider::Claude, Some(first)),
        },
        [] => (Provider::Claude, None),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn should_read_the_provider_first_and_default_to_claude() {
        assert_eq!(
            registration(&args(&["codex", "Stop"])),
            (Provider::Codex, Some("Stop"))
        );
        assert_eq!(
            registration(&args(&["Stop"])),
            (Provider::Claude, Some("Stop"))
        );
        assert_eq!(
            registration(&args(&["claude", "statusline"])),
            (Provider::Claude, Some("statusline"))
        );
        assert_eq!(registration(&args(&[])), (Provider::Claude, None));
    }
}
