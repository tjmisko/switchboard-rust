//! T02 — YOUR tests for `interpret_reply` and `client::send`.
//!
//! Write each body before implementing the function it tests, and watch it
//! fail. The doc comment on each stub says what it must pin down. At the bottom,
//! add at least two tests for cases this list does not name. The hidden golden
//! suite has some.

use std::error::Error;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::json;

use switchboard::client::send;
use switchboard::error::ForwardError;
use switchboard::ingest::forward::interpret_reply;

/// A socket path short enough for `SUN_LEN` (108 bytes) and unique per test.
fn socket(name: &str) -> PathBuf {
    let p = std::env::temp_dir().join(format!("sbt-{}-{name}.sock", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// A one-shot daemon: accept one connection, read one line, then reply with
/// `reply`, or with nothing for longer than any sensible timeout if `None`.
/// Returns the line it received.
fn one_shot(path: &PathBuf, reply: Option<&'static str>) -> thread::JoinHandle<String> {
    let listener = UnixListener::bind(path).unwrap();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        match reply {
            Some(r) => {
                (&stream).write_all(r.as_bytes()).unwrap();
                (&stream).write_all(b"\n").unwrap();
            }
            None => thread::sleep(Duration::from_millis(400)),
        }
        line
    })
}

/// A daemon that accepts and hangs up without reading or writing anything.
fn hangs_up(path: &PathBuf) -> thread::JoinHandle<()> {
    let listener = UnixListener::bind(path).unwrap();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        drop(stream);
    })
}

// ---------------------------------------------------------------------------
// interpret_reply: the four ways a reply can be classified
// ---------------------------------------------------------------------------

/// `{"ok":true}` is success.
#[test]
fn should_accept_ok_true() {
    assert!(interpret_reply(r#"{"ok":true}"#).is_ok());
    // Extra fields are not a verdict either way.
    assert!(interpret_reply(r#"{"ok":true,"snapshot":{}}"#).is_ok());
}

/// `ok: false` is `Rejected`, carrying the daemon's `error` string. When
/// `error` is absent, the reason is still present and says so.
#[test]
fn should_classify_ok_false_as_rejected_with_the_reason() {
    let err = interpret_reply(r#"{"ok":false,"error":"missing field `cmd`"}"#).unwrap_err();
    let ForwardError::Rejected(reason) = &err else {
        panic!("expected Rejected, got {err:?}");
    };
    assert_eq!(&**reason, "missing field `cmd`");
    assert!(err.source().is_none(), "a refusal has nothing underneath it");

    let bare = interpret_reply(r#"{"ok":false}"#).unwrap_err();
    let ForwardError::Rejected(reason) = &bare else {
        panic!("expected Rejected, got {bare:?}");
    };
    assert!(!reason.is_empty(), "an absent reason is still a reason");

    // An `error` that is not a string is treated like an absent one.
    let odd = interpret_reply(r#"{"ok":false,"error":42}"#).unwrap_err();
    assert!(matches!(odd, ForwardError::Rejected(r) if !r.is_empty()));
}

/// JSON without a *boolean* `ok` is `NoVerdict`. Think about what else could
/// be sitting in the `ok` field.
#[test]
fn should_classify_a_reply_without_a_boolean_ok_as_no_verdict() {
    for line in [
        r#"{}"#,
        r#"{"snapshot":{}}"#,
        r#"{"ok":"true"}"#,
        r#"{"ok":1}"#,
        r#"{"ok":null}"#,
        r#"{"ok":[true]}"#,
        r#"{"OK":true}"#,
        r#"[true]"#,
        r#"true"#,
        r#"null"#,
    ] {
        assert!(
            matches!(interpret_reply(line), Err(ForwardError::NoVerdict)),
            "{line}"
        );
    }
}

// ---------------------------------------------------------------------------
// send: transport failures are not replies
// ---------------------------------------------------------------------------

/// Nothing is listening: `Connect`, not `Io`.
#[test]
fn should_report_connect_when_no_daemon_is_listening() {
    let path = socket("nobody");
    let err = send(&path, &json!({"cmd": "list"}), Duration::from_millis(200)).unwrap_err();
    assert!(matches!(err, ForwardError::Connect(_)), "{err:?}");
    assert!(err.source().is_some(), "the io error is the source");
}

/// The daemon receives exactly one line, newline-terminated and valid JSON,
/// and an `ok` reply makes `send` return `Ok`.
#[test]
fn should_send_one_newline_terminated_frame_and_accept_ok() {
    let path = socket("ok");
    let daemon = one_shot(&path, Some(r#"{"ok":true}"#));
    let frame = json!({"cmd": "hook", "event": "Stop", "pid": 7, "session_id": "s-1"});

    send(&path, &frame, Duration::from_secs(1)).expect("ok reply is Ok");

    let received = daemon.join().unwrap();
    assert!(received.ends_with('\n'), "frame must be newline-terminated");
    assert_eq!(received.matches('\n').count(), 1, "exactly one line");
    let parsed: serde_json::Value = serde_json::from_str(received.trim_end()).unwrap();
    assert_eq!(parsed, frame);
}

/// A daemon that accepts but never answers: `Io`, after the timeout.
#[test]
fn should_report_io_when_the_daemon_never_answers_within_the_timeout() {
    let path = socket("silent");
    let daemon = one_shot(&path, None);
    let started = Instant::now();

    let err = send(&path, &json!({"cmd": "list"}), Duration::from_millis(100)).unwrap_err();

    let elapsed = started.elapsed();
    assert!(matches!(err, ForwardError::Io(_)), "{err:?}");
    assert!(
        elapsed < Duration::from_millis(350),
        "returned after {elapsed:?}: the timeout fired, not the daemon's hang-up"
    );
    daemon.join().unwrap();
}

/// A live daemon's refusal surfaces as `Rejected`, and its `Display` reads
/// well enough to print from `SWITCHBOARD_DEBUG`.
#[test]
fn should_surface_a_refusal_from_a_live_daemon() {
    let path = socket("refuse");
    let daemon = one_shot(&path, Some(r#"{"ok":false,"error":"unknown command: focus"}"#));

    let err = send(&path, &json!({"cmd": "focus"}), Duration::from_secs(1)).unwrap_err();

    assert!(
        matches!(&err, ForwardError::Rejected(r) if &**r == "unknown command: focus"),
        "{err:?}"
    );
    let shown = err.to_string();
    assert!(shown.contains("unknown command: focus"), "{shown}");
    assert!(
        !shown.contains("Rejected(") && !shown.contains("Box<"),
        "Display must not be Debug: {shown}"
    );
    daemon.join().unwrap();
}

// ---------------------------------------------------------------------------
// yours: at least two more
// ---------------------------------------------------------------------------

/// A reply that is not JSON at all is `Malformed`, with the serde error
/// underneath, and it is not confused with a refusal or a missing verdict.
#[test]
fn should_classify_a_non_json_reply_as_malformed_with_its_source() {
    for line in ["", "ok", "{not json", r#"{"ok":true"#] {
        let err = interpret_reply(line).unwrap_err();
        assert!(matches!(err, ForwardError::Malformed(_)), "{line:?}: {err:?}");
        assert!(err.source().is_some(), "{line:?}: the serde error is the source");
    }
}

/// A daemon that accepts the connection and closes it without a byte is a
/// transport failure (`Io`), not a malformed reply: no reply ever arrived.
#[test]
fn should_report_io_when_the_daemon_hangs_up_without_replying() {
    let path = socket("hangup");
    let daemon = hangs_up(&path);

    let err = send(&path, &json!({"cmd": "list"}), Duration::from_secs(1)).unwrap_err();

    assert!(matches!(err, ForwardError::Io(_)), "{err:?}");
    daemon.join().unwrap();
}

/// The four classes render as four different messages, and only the three
/// that wrap another error expose a source.
#[test]
fn should_render_every_variant_distinctly_and_chain_only_the_wrapped_ones() {
    let io = || std::io::Error::other("boom");
    let serde = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
    let errs = [
        ForwardError::Connect(io()),
        ForwardError::Io(io()),
        ForwardError::Malformed(serde),
        ForwardError::NoVerdict,
        ForwardError::Rejected("nope".into()),
    ];
    let shown: Vec<String> = errs.iter().map(ToString::to_string).collect();
    for (i, a) in shown.iter().enumerate() {
        assert!(!a.is_empty());
        for b in &shown[i + 1..] {
            assert_ne!(a, b, "two variants share a message");
        }
    }
    let sourced: Vec<bool> = errs.iter().map(|e| e.source().is_some()).collect();
    assert_eq!(sourced, [true, true, true, false, false]);
}
