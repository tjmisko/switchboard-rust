//! T03 — JSON navigation, the input hash, scrubbing, and the hook adapter.

mod common;

use std::time::Instant;

use serde_json::{Value, json};

use common::{fixture_json, key};
use switchboard::ingest::forward::scrub;
use switchboard::ingest::frame::{Request, decode_request};
use switchboard::ingest::hash::input_hash;
use switchboard::ingest::json::{array_at, at, bool_at, nonblank_str_at, str_at, str_path, u64_at};
use switchboard::ingest::{Ctx, Input, claude};
use switchboard::model::*;

// ---------------------------------------------------------------------------
// json helpers
// ---------------------------------------------------------------------------

#[test]
fn should_read_typed_scalars_and_return_none_for_absent_or_mistyped() {
    let v = json!({"s": "x", "blank": "  ", "n": 7, "neg": -1, "b": true});
    assert_eq!(str_at(&v, "s"), Some("x"));
    assert_eq!(str_at(&v, "n"), None);
    assert_eq!(str_at(&v, "missing"), None);
    assert_eq!(nonblank_str_at(&v, "blank"), None);
    assert_eq!(nonblank_str_at(&v, "s"), Some("x"));
    assert_eq!(u64_at(&v, "n"), Some(7));
    assert_eq!(u64_at(&v, "neg"), None);
    assert_eq!(bool_at(&v, "b"), Some(true));
    assert_eq!(bool_at(&v, "s"), None);
}

#[test]
fn should_walk_a_path_and_stop_at_the_first_missing_or_non_object_step() {
    let v = json!({"message": {"usage": {"input_tokens": 3}}, "list": [1]});
    assert_eq!(
        at(&v, &["message", "usage", "input_tokens"]).and_then(Value::as_u64),
        Some(3)
    );
    assert_eq!(at(&v, &["message", "nope", "input_tokens"]), None);
    assert_eq!(
        at(&v, &["list", "0"]),
        None,
        "arrays are not indexed by key"
    );
    assert_eq!(
        str_path(&json!({"origin": {"kind": "human"}}), &["origin", "kind"]),
        Some("human")
    );
}

#[test]
fn should_iterate_array_elements_or_nothing() {
    let v = json!({"a": [1, 2, 3], "s": "x"});
    assert_eq!(array_at(&v, "a").count(), 3);
    assert_eq!(array_at(&v, "s").count(), 0);
    assert_eq!(array_at(&v, "none").count(), 0);
}

#[test]
fn should_return_a_str_that_borrows_from_the_value_not_from_the_key() {
    // The key is a temporary; the result outlives it because `'a` ties the
    // result to `v` alone.
    let v = json!({"session_id": "s-1"});
    let found = {
        let k = String::from("session_id");
        str_at(&v, &k)
    };
    assert_eq!(found, Some("s-1"));
}

// ---------------------------------------------------------------------------
// hash
// ---------------------------------------------------------------------------

#[test]
fn should_hash_the_same_input_identically_from_a_hook_and_a_transcript() {
    // PermissionRequest's tool_input, and the transcript's tool_use input for
    // the same call, with keys in different orders.
    let from_hook = fixture_json("claude/hooks/permission_request.json")["tool_input"].clone();
    let from_transcript = json!({"command": "git status", "description": "check"});
    assert_eq!(input_hash(&from_hook), input_hash(&from_transcript));
}

#[test]
fn should_hash_nested_key_order_away() {
    let a: Value = serde_json::from_str(r#"{"q":[{"b":1,"a":2}],"z":null}"#).unwrap();
    let b: Value = serde_json::from_str(r#"{"z":null,"q":[{"a":2,"b":1}]}"#).unwrap();
    assert_eq!(input_hash(&a), input_hash(&b));
    let reordered_array: Value = serde_json::from_str(r#"{"z":null,"q":[{"a":1,"b":2}]}"#).unwrap();
    assert_ne!(input_hash(&a), input_hash(&reordered_array));
}

#[test]
fn should_round_trip_hex_and_reject_wrong_lengths_and_digits() {
    let h = input_hash(&json!({"command": "ls"}));
    let hex = h.to_hex();
    assert_eq!(hex.len(), 16);
    assert!(
        hex.bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    );
    assert_eq!(InputHash::from_hex(&hex), Some(h));
    assert_eq!(InputHash::from_hex(&hex.to_uppercase()), Some(h));
    assert_eq!(InputHash::from_hex(&hex[..15]), None);
    assert_eq!(InputHash::from_hex(&format!("{hex}0")), None);
    assert_eq!(InputHash::from_hex("g000000000000000"), None);
    assert_eq!(InputHash::from_hex("+f00000000000000"), None);
}

// ---------------------------------------------------------------------------
// scrub
// ---------------------------------------------------------------------------

#[test]
fn should_drop_all_content_and_keep_identity_when_scrubbing() {
    let raw = fixture_json("claude/hooks/post_tool_use_subagent.json");
    let expected_hash = input_hash(&raw["tool_input"]).to_hex();
    let out = scrub(raw, Some("PostToolUse"), 4036);

    for field in [
        "tool_input",
        "tool_response",
        "prompt",
        "last_assistant_message",
    ] {
        assert!(out.get(field).is_none(), "{field} leaked");
    }
    assert_eq!(out["tool_input_hash"], expected_hash);
    assert_eq!(out["cmd"], "hook");
    assert_eq!(out["event"], "PostToolUse");
    assert_eq!(out["pid"], 4036);
    assert_eq!(out["tool_use_id"], "toolu_01XYZ");
    assert_eq!(out["agent_id"], "ac24f297af282c150");
}

#[test]
fn should_fall_back_to_hook_event_name_when_argv_has_no_event() {
    let raw = json!({"hook_event_name": "Stop", "session_id": "s"});
    assert_eq!(scrub(raw, None, 1)["event"], "Stop");
}

#[test]
fn should_still_produce_a_frame_when_stdin_was_not_an_object() {
    let out = scrub(Value::Null, Some("Stop"), 9);
    assert_eq!(out, json!({"cmd": "hook", "event": "Stop", "pid": 9}));
}

// ---------------------------------------------------------------------------
// the adapter, end to end from a raw payload
// ---------------------------------------------------------------------------

/// raw stdin -> scrub -> socket line -> decode -> adapter
fn through_socket(fixture: &str, event: &str) -> Vec<Input> {
    let raw = fixture_json(&format!("claude/hooks/{fixture}"));
    let line = scrub(raw, Some(event), 4036).to_string();
    let Request::Hook(frame) = decode_request(&line).expect("decodes") else {
        panic!("not a hook frame");
    };
    let ctx = Ctx {
        now: Instant::now(),
        process: Some(key(4036)),
    };
    claude::hook(&frame, &ctx)
}

fn only_event(inputs: Vec<Input>) -> Event {
    let mut events: Vec<Event> = inputs
        .into_iter()
        .filter_map(|i| match i {
            Input::Event(e) => Some(e),
            Input::Snapshot(_) => None,
        })
        .collect();
    assert_eq!(events.len(), 1, "expected exactly one event: {events:?}");
    events.remove(0)
}

#[test]
fn should_start_a_session_caused_by_the_user_when_source_is_clear() {
    let e = only_event(through_socket("session_start_clear.json", "SessionStart"));
    assert_eq!(e.cause, Cause::User);
    assert_eq!(e.confidence, Confidence::Confirmed);
    assert_eq!(e.process, Some(key(4036)));
    assert_eq!(e.session.as_str(), "e0b4b21f-aaf6-4ab0-a8d6-2d595aba4065");
    match e.kind {
        EventKind::SessionStarted { source, title } => {
            assert_eq!(source, StartSource::Clear);
            assert_eq!(title.as_deref(), Some("switchboard-rust-7f"));
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_key_a_typed_prompt_by_prompt_id_and_attribute_it_to_the_user() {
    let e = only_event(through_socket(
        "user_prompt_submit.json",
        "UserPromptSubmit",
    ));
    assert_eq!(e.cause, Cause::User);
    assert_eq!(e.actor, Actor::Agent);
    match e.kind {
        EventKind::TurnStarted { turn, origin } => {
            assert_eq!(
                turn,
                Some(TurnKey::Prompt(
                    "9b1c2d3e-4f50-4a61-8b72-93a4b5c6d7e8".into()
                ))
            );
            assert_eq!(origin, Origin::Human);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_attribute_a_loop_wakeup_prompt_to_the_agent() {
    let e = only_event(through_socket(
        "user_prompt_submit_loop_wakeup.json",
        "UserPromptSubmit",
    ));
    assert_eq!(e.cause, Cause::Agent);
    assert!(matches!(
        e.kind,
        EventKind::TurnStarted {
            turn: None,
            origin: Origin::Machine
        }
    ));
}

#[test]
fn should_dispatch_a_tool_with_the_forwarders_hash() {
    let raw = fixture_json("claude/hooks/pre_tool_use_bash.json");
    let expected = input_hash(&raw["tool_input"]);
    let e = only_event(through_socket("pre_tool_use_bash.json", "PreToolUse"));
    assert_eq!(e.cause, Cause::Agent);
    match e.kind {
        EventKind::ToolDispatched { call, tool, hash } => {
            assert_eq!(call.as_str(), "toolu_01ABC");
            assert_eq!(tool.as_str(), "Bash");
            assert_eq!(hash, expected);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_turn_a_question_tools_pre_tool_use_into_a_question_gate_from_the_subagent() {
    let e = only_event(through_socket(
        "pre_tool_use_ask_subagent.json",
        "PreToolUse",
    ));
    assert_eq!(e.actor, Actor::Subagent(SubagentId::from("agent-child")));
    match e.kind {
        EventKind::PermissionAsked {
            call,
            tool,
            owner,
            kind,
            ..
        } => {
            assert_eq!(call.as_deref(), Some("toolu_ask"));
            assert_eq!(tool.as_str(), "AskUserQuestion");
            assert_eq!(owner, Ownership::Human);
            assert_eq!(kind, GateKind::Question);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_ask_permission_without_a_call_id_for_claude() {
    let raw = fixture_json("claude/hooks/permission_request.json");
    let expected = input_hash(&raw["tool_input"]);
    let e = only_event(through_socket(
        "permission_request.json",
        "PermissionRequest",
    ));
    match e.kind {
        EventKind::PermissionAsked {
            call,
            tool,
            hash,
            owner,
            kind,
        } => {
            assert_eq!(call, None);
            assert_eq!(tool.as_str(), "Bash");
            assert_eq!(hash, Some(expected));
            assert_eq!(owner, Ownership::Human);
            assert_eq!(kind, GateKind::Approval);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_settle_a_subagents_tool_with_its_duration() {
    let e = only_event(through_socket("post_tool_use_subagent.json", "PostToolUse"));
    assert_eq!(
        e.actor,
        Actor::Subagent(SubagentId::from("ac24f297af282c150"))
    );
    assert!(matches!(
        e.kind,
        EventKind::ToolSettled {
            ref call,
            outcome: ToolOutcome::Ok { ms: Some(412) }
        } if call.as_str() == "toolu_01XYZ"
    ));
}

#[test]
fn should_attribute_an_interrupted_tool_failure_to_the_user() {
    let e = only_event(through_socket(
        "post_tool_use_failure_interrupt.json",
        "PostToolUseFailure",
    ));
    assert_eq!(e.cause, Cause::User);
    assert!(matches!(
        e.kind,
        EventKind::ToolSettled {
            outcome: ToolOutcome::Failed { interrupt: true },
            ..
        }
    ));
}

#[test]
fn should_end_the_turn_and_snapshot_only_live_waits_when_stop_reports_them() {
    let inputs = through_socket("stop_with_waits.json", "Stop");
    assert_eq!(inputs.len(), 2);
    let Input::Event(e) = &inputs[0] else {
        panic!("event first");
    };
    assert!(matches!(
        e.kind,
        EventKind::TurnEnded {
            turn: Some(_),
            outcome: TurnOutcome::Completed
        }
    ));
    let Input::Snapshot(Snapshot {
        kind: SnapshotKind::Waits { rows, .. },
        ..
    }) = &inputs[1]
    else {
        panic!("waits snapshot second");
    };
    let got: Vec<(&str, WaitKind)> = rows.iter().map(|r| (r.id.0.as_ref(), r.kind)).collect();
    assert_eq!(
        got,
        [
            ("bg-1", WaitKind::BackgroundShell),
            ("bg-2", WaitKind::Monitor),
            ("cron-1", WaitKind::Cron),
        ]
    );
}

#[test]
fn should_not_snapshot_waits_when_stop_carries_neither_key() {
    let inputs = through_socket("stop_bare.json", "Stop");
    assert_eq!(inputs.len(), 1);
}

#[test]
fn should_classify_a_stop_failure() {
    let e = only_event(through_socket(
        "stop_failure_rate_limit.json",
        "StopFailure",
    ));
    assert!(matches!(
        e.kind,
        EventKind::TurnEnded {
            outcome: TurnOutcome::Failed {
                class: ErrorClass::RateLimit
            },
            ..
        }
    ));
}

#[test]
fn should_attribute_subagent_lifecycle_to_the_agent_and_name_the_child() {
    let start = only_event(through_socket("subagent_start.json", "SubagentStart"));
    assert_eq!(start.actor, Actor::Agent);
    assert!(matches!(
        start.kind,
        EventKind::SubagentStarted { ref subagent, .. } if subagent.as_str() == "a1a4f5cc80cc6d9c2"
    ));
    let stop = only_event(through_socket("subagent_stop.json", "SubagentStop"));
    assert_eq!(stop.actor, Actor::Agent);
    assert!(matches!(stop.kind, EventKind::SubagentEnded { .. }));
}

#[test]
fn should_carry_the_new_cwd_and_a_manual_compaction_by_the_user() {
    let cwd = only_event(through_socket("cwd_changed.json", "CwdChanged"));
    assert!(matches!(
        cwd.kind,
        EventKind::CwdChanged { ref to } if to.ends_with(".worktrees/golden")
    ));
    let compact = only_event(through_socket("pre_compact_manual.json", "PreCompact"));
    assert_eq!(compact.cause, Cause::User);
}

#[test]
fn should_produce_nothing_for_an_event_the_model_does_not_track() {
    assert!(through_socket("notification_idle.json", "Notification").is_empty());
    assert!(through_socket("stop_bare.json", "SomeFutureHook").is_empty());
}

#[test]
fn should_produce_nothing_when_a_tool_event_lacks_its_call_id() {
    let mut raw = fixture_json("claude/hooks/pre_tool_use_bash.json");
    raw.as_object_mut().unwrap().remove("tool_use_id");
    let line = scrub(raw, Some("PreToolUse"), 1).to_string();
    let Request::Hook(frame) = decode_request(&line).unwrap() else {
        panic!()
    };
    let ctx = Ctx {
        now: Instant::now(),
        process: None,
    };
    assert!(claude::hook(&frame, &ctx).is_empty());
}
