//! T02 (golden, hidden) — the forwarder's reply classification and transport.
//! The learner writes their own version of these tests in `t02_forward.rs`.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use serde_json::json;

use switchboard::client::send;
use switchboard::error::ForwardError;
use switchboard::ingest::forward::interpret_reply;

fn socket(name: &str) -> PathBuf {
    // Short: SUN_LEN is 108 bytes.
    let dir = std::env::temp_dir();
    let p = dir.join(format!("sbg-{}-{name}.sock", std::process::id()));
    let _ = std::fs::remove_file(&p);
    p
}

/// A one-shot daemon that reads one line and replies with `reply`.
fn one_shot(path: &PathBuf, reply: Option<&'static str>) -> thread::JoinHandle<String> {
    let listener = UnixListener::bind(path).unwrap();
    thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).unwrap();
        if let Some(reply) = reply {
            (&stream).write_all(reply.as_bytes()).unwrap();
            (&stream).write_all(b"\n").unwrap();
        } else {
            thread::sleep(Duration::from_millis(400));
        }
        line
    })
}

#[test]
fn should_accept_ok_true() {
    assert!(interpret_reply(r#"{"ok":true}"#).is_ok());
}

#[test]
fn should_classify_ok_false_as_rejected_with_the_reason() {
    match interpret_reply(r#"{"ok":false,"error":"missing field `cmd`"}"#) {
        Err(ForwardError::Rejected(why)) => assert_eq!(&*why, "missing field `cmd`"),
        other => panic!("{other:?}"),
    }
    match interpret_reply(r#"{"ok":false}"#) {
        Err(ForwardError::Rejected(why)) => assert_eq!(&*why, "no reason given"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_classify_a_reply_without_a_boolean_ok_as_no_verdict() {
    for reply in [r#"{}"#, r#"{"ok":"true"}"#, r#"{"snapshot":{}}"#] {
        assert!(
            matches!(interpret_reply(reply), Err(ForwardError::NoVerdict)),
            "{reply}"
        );
    }
}

#[test]
fn should_classify_non_json_and_empty_replies_as_malformed() {
    assert!(matches!(
        interpret_reply("OK"),
        Err(ForwardError::Malformed(_))
    ));
    assert!(matches!(
        interpret_reply(""),
        Err(ForwardError::Malformed(_))
    ));
}

#[test]
fn should_report_connect_when_no_daemon_is_listening() {
    let path = socket("nobody");
    let err = send(&path, &json!({"cmd":"hook"}), Duration::from_millis(100)).unwrap_err();
    assert!(matches!(err, ForwardError::Connect(_)), "{err:?}");
}

#[test]
fn should_send_one_newline_terminated_frame_and_accept_ok() {
    let path = socket("ok");
    let server = one_shot(&path, Some(r#"{"ok":true}"#));
    send(
        &path,
        &json!({"cmd":"hook","event":"Stop"}),
        Duration::from_secs(1),
    )
    .unwrap();
    let received = server.join().unwrap();
    assert!(received.ends_with('\n'));
    let v: serde_json::Value = serde_json::from_str(received.trim_end()).unwrap();
    assert_eq!(v["event"], "Stop");
    let _ = std::fs::remove_file(&path);
}

#[test]
fn should_report_io_when_the_daemon_never_answers_within_the_timeout() {
    let path = socket("slow");
    let server = one_shot(&path, None);
    let err = send(&path, &json!({"cmd":"hook"}), Duration::from_millis(100)).unwrap_err();
    assert!(matches!(err, ForwardError::Io(_)), "{err:?}");
    server.join().unwrap();
    let _ = std::fs::remove_file(&path);
}

#[test]
fn should_surface_a_refusal_from_a_live_daemon() {
    let path = socket("refuse");
    let server = one_shot(&path, Some(r#"{"ok":false,"error":"unknown command: x"}"#));
    let err = send(&path, &json!({"cmd":"x"}), Duration::from_secs(1)).unwrap_err();
    assert!(matches!(err, ForwardError::Rejected(_)));
    assert_eq!(
        err.to_string(),
        "daemon refused the frame: unknown command: x"
    );
    server.join().unwrap();
    let _ = std::fs::remove_file(&path);
}
