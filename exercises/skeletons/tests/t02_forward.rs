//! T02 — YOUR tests for `interpret_reply` and `client::send`.
//!
//! Write each body before implementing the function it tests, and watch it
//! fail. The doc comment on each stub says what it must pin down. At the bottom,
//! add at least two tests for cases this list does not name. The hidden golden
//! suite has some.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use switchboard::client::send;
use switchboard::error::ForwardError;
use switchboard::ingest::forward::interpret_reply;

/// How long the silent daemon holds a connection open before giving up on
/// it. Far longer than any timeout you would pass to `send`, on purpose: a
/// `send` that never installs a read timeout sits here for all five seconds,
/// and the timeout test must notice that.
const SILENT_FOR: Duration = Duration::from_secs(5);

/// A socket path short enough for `SUN_LEN` (108 bytes) and unique per test.
fn socket(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("sbt-{}-{name}.sock", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// A one-shot daemon: accept one connection, read one line, then reply with
/// `reply`. With `None` it replies with nothing and keeps the connection open
/// until the client hangs up or `SILENT_FOR` passes, whichever comes first.
/// Returns the line it received.
fn one_shot(path: &PathBuf, reply: Option<&'static str>) -> thread::JoinHandle<String> {
    let listener = UnixListener::bind(path).unwrap();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(&stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        match reply {
            Some(r) => {
                (&stream).write_all(r.as_bytes()).unwrap();
                (&stream).write_all(b"\n").unwrap();
            }
            None => {
                // Wait for a second line that never comes. A client that
                // times out drops its stream, and that ends this read at once.
                stream.set_read_timeout(Some(SILENT_FOR)).unwrap();
                let _ = reader.read_line(&mut String::new());
            }
        }
        line
    })
}

// ---------------------------------------------------------------------------
// interpret_reply: the four ways a reply can be classified
// ---------------------------------------------------------------------------

/// `{"ok":true}` is success.
#[test]
fn should_accept_ok_true() {
    todo!("write this test")
}

/// `ok: false` is `Rejected`, carrying the daemon's `error` string. When
/// `error` is absent or not a string, the reason is exactly `no reason given`
/// (the hidden suite pins that text).
#[test]
fn should_classify_ok_false_as_rejected_with_the_reason() {
    todo!("write this test")
}

/// JSON without a *boolean* `ok` is `NoVerdict`. Think about what else could
/// be sitting in the `ok` field.
#[test]
fn should_classify_a_reply_without_a_boolean_ok_as_no_verdict() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// send: transport failures are not replies
// ---------------------------------------------------------------------------

/// Nothing is listening: `Connect`, not `Io`.
#[test]
fn should_report_connect_when_no_daemon_is_listening() {
    todo!("write this test")
}

/// The daemon receives exactly one line, newline-terminated and valid JSON,
/// and an `ok` reply makes `send` return `Ok`.
#[test]
fn should_send_one_newline_terminated_frame_and_accept_ok() {
    todo!("write this test")
}

/// A daemon that accepts but never answers: `Io`, after the timeout. Take an
/// `Instant` before the call and assert that `send` came back well before
/// `SILENT_FOR`. Without that assertion, a `send` that never calls
/// `set_read_timeout` still passes: it blocks until the daemon hangs up, and
/// that hang-up also comes out as an error.
#[test]
fn should_report_io_when_the_daemon_never_answers_within_the_timeout() {
    todo!("write this test")
}

/// A live daemon's refusal surfaces as `Rejected`, and its `Display` reads
/// well enough to print from `SWITCHBOARD_DEBUG`: the hidden suite pins
/// `daemon refused the frame: <reason>`, exactly.
#[test]
fn should_surface_a_refusal_from_a_live_daemon() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// yours: at least two more
// ---------------------------------------------------------------------------
