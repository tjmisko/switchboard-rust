//! T02 — error architecture: vocabularies, frame decoding, error chains.

use std::error::Error;

use switchboard::error::{FrameError, RecordError, StatError, UnknownName};
use switchboard::ingest::frame::{Request, decode_request, error_reply, ok_reply};
use switchboard::model::{Denier, Phase, StartSource};

// ---------------------------------------------------------------------------
// Vocabularies
// ---------------------------------------------------------------------------

#[test]
fn should_parse_every_phase_from_the_name_it_prints() {
    for p in Phase::ALL {
        assert_eq!(p.to_string().parse::<Phase>(), Ok(p));
    }
}

#[test]
fn should_name_the_vocabulary_and_value_when_a_phase_is_unknown() {
    let err = "permission".parse::<Phase>().unwrap_err();
    assert_eq!(err, UnknownName::new("phase", "permission"));
    assert_eq!(err.to_string(), "unknown phase: \"permission\"");
}

#[test]
fn should_parse_start_sources_and_denial_kinds_from_their_wire_names() {
    assert_eq!("clear".parse(), Ok(StartSource::Clear));
    assert_eq!("resume".parse(), Ok(StartSource::Resume));
    assert!("reboot".parse::<StartSource>().is_err());

    assert_eq!("user-rejected".parse(), Ok(Denier::User));
    assert_eq!("automode-blocked".parse(), Ok(Denier::AutoMode));
    assert_eq!("permission-rule".parse(), Ok(Denier::PermissionRule));
    assert!("User-Rejected".parse::<Denier>().is_err());
}

// ---------------------------------------------------------------------------
// Frames: absence vs malformation
// ---------------------------------------------------------------------------

#[test]
fn should_decode_list_and_subscribe_from_cmd_alone() {
    assert_eq!(decode_request(r#"{"cmd":"list"}"#).unwrap(), Request::List);
    assert_eq!(
        decode_request(r#"{"cmd":"subscribe","extra":1}"#).unwrap(),
        Request::Subscribe
    );
}

#[test]
fn should_carry_event_pid_and_session_when_decoding_a_hook() {
    let line = r#"{"cmd":"hook","event":"Stop","pid":4036,"session_id":"s-1","x":true}"#;
    let Request::Hook(frame) = decode_request(line).unwrap() else {
        panic!("not a hook");
    };
    assert_eq!(&*frame.event, "Stop");
    assert_eq!(frame.pid, Some(4036));
    assert_eq!(frame.session.as_str(), "s-1");
    assert_eq!(frame.payload["x"], true);
}

#[test]
fn should_report_malformed_json_with_its_source_attached() {
    let err = decode_request("{not json").unwrap_err();
    assert!(matches!(err, FrameError::Json(_)));
    assert!(err.source().is_some(), "the serde error is the source");
    assert!(err.to_string().starts_with("malformed JSON: "));
}

#[test]
fn should_reject_json_that_is_not_an_object() {
    for line in ["[1,2]", "\"list\"", "42", "null"] {
        assert!(
            matches!(decode_request(line), Err(FrameError::NotAnObject)),
            "{line}"
        );
    }
}

#[test]
fn should_distinguish_a_missing_field_from_a_mistyped_one() {
    assert!(matches!(
        decode_request(r#"{"event":"Stop"}"#),
        Err(FrameError::Missing("cmd"))
    ));
    assert!(matches!(
        decode_request(r#"{"cmd":7}"#),
        Err(FrameError::WrongType { field: "cmd", .. })
    ));
    assert!(matches!(
        decode_request(r#"{"cmd":"hook","session_id":"s"}"#),
        Err(FrameError::Missing("event"))
    ));
    assert!(matches!(
        decode_request(r#"{"cmd":"hook","event":"Stop"}"#),
        Err(FrameError::Missing("session_id"))
    ));
    assert!(matches!(
        decode_request(r#"{"cmd":"hook","event":"Stop","session_id":null}"#),
        Err(FrameError::WrongType {
            field: "session_id",
            ..
        })
    ));
}

#[test]
fn should_accept_a_missing_pid_but_reject_one_that_is_not_a_u32() {
    let ok = r#"{"cmd":"statusline","session_id":"s"}"#;
    assert!(matches!(
        decode_request(ok),
        Ok(Request::Statusline(f)) if f.pid.is_none()
    ));

    for bad in [
        r#"{"cmd":"hook","event":"Stop","session_id":"s","pid":"4036"}"#,
        r#"{"cmd":"hook","event":"Stop","session_id":"s","pid":-1}"#,
        r#"{"cmd":"hook","event":"Stop","session_id":"s","pid":4294967296}"#,
        r#"{"cmd":"hook","event":"Stop","session_id":"s","pid":1.5}"#,
    ] {
        assert!(
            matches!(
                decode_request(bad),
                Err(FrameError::WrongType { field: "pid", .. })
            ),
            "{bad}"
        );
    }
}

#[test]
fn should_name_the_command_when_it_is_unknown() {
    let err = decode_request(r#"{"cmd":"focus"}"#).unwrap_err();
    assert!(matches!(&err, FrameError::UnknownCommand(c) if &**c == "focus"));
    assert_eq!(err.to_string(), "unknown command: focus");
    assert!(err.source().is_none());
}

#[test]
fn should_render_replies_as_single_line_json() {
    assert_eq!(ok_reply(), r#"{"ok":true}"#);
    let reply: serde_json::Value =
        serde_json::from_str(&error_reply(&FrameError::Missing("cmd"))).unwrap();
    assert_eq!(reply["ok"], false);
    assert_eq!(reply["error"], "missing field `cmd`");
}

// ---------------------------------------------------------------------------
// Error chains
// ---------------------------------------------------------------------------

#[test]
fn should_expose_the_parse_error_as_the_source_of_a_bad_stat_number() {
    let source = "x".parse::<u64>().unwrap_err();
    let err = StatError::BadNumber {
        field: "starttime",
        source,
    };
    assert!(err.to_string().contains("starttime"));
    let inner = err.source().expect("has a source");
    assert_eq!(inner.to_string(), "invalid digit found in string");
}

#[test]
fn should_convert_with_question_mark_into_a_record_error() {
    fn parse(line: &str) -> Result<Phase, RecordError> {
        let v: serde_json::Value = serde_json::from_str(line)?; // From<serde_json::Error>
        let name = v["to"].as_str().ok_or(RecordError::Missing("to"))?;
        Ok(name.parse::<Phase>()?) // From<UnknownName>
    }
    assert_eq!(parse(r#"{"to":"idle"}"#).unwrap(), Phase::Idle);
    assert!(matches!(parse("nope"), Err(RecordError::Json(_))));
    assert!(matches!(parse("{}"), Err(RecordError::Missing("to"))));
    let name = parse(r#"{"to":"permission"}"#).unwrap_err();
    assert!(matches!(name, RecordError::Name(_)));
    assert!(name.source().is_some());
}

#[test]
fn should_box_any_of_these_errors_into_a_send_sync_error() {
    fn boxed() -> Result<(), switchboard::error::BoxError> {
        decode_request("[]")?;
        Ok(())
    }
    let err = boxed().unwrap_err();
    assert_eq!(err.to_string(), "frame is not a JSON object");
}
