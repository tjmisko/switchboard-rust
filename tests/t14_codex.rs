//! The Codex hook adapter, and the stories it produces through the reducer.
//!
//! Every fixture under `tests/fixtures/codex/hooks/` is the shape captured
//! from Codex 0.154 in September 2026, with content filled in. Frames go
//! through the forwarder's Codex scrub, the socket decoder, the adapter, and
//! then the reducer, exactly as the daemon does it.

mod common;

use std::time::Instant;

use serde_json::{Value, json};

use common::{Clock, agent, fixture_json, key, publishes, shown, truth};
use switchboard::ingest::forward::scrub_for;
use switchboard::ingest::frame::{Request, decode_request};
use switchboard::ingest::hash::input_hash;
use switchboard::ingest::{Ctx, Input, codex};
use switchboard::model::*;
use switchboard::reduce;

const PID: u32 = 4036;

/// raw stdin -> Codex scrub -> socket line -> decode -> adapter
fn adapt(fixture: &str, event: &str, at: Instant) -> Vec<Input> {
    let raw = fixture_json(&format!("codex/hooks/{fixture}"));
    let line = scrub_for(Provider::Codex, raw, Some(event), PID).to_string();
    let Request::Hook(frame) = decode_request(&line).expect("decodes") else {
        panic!("not a hook frame");
    };
    assert_eq!(
        frame.provider,
        Provider::Codex,
        "the frame names its provider"
    );
    codex::hook(
        &frame,
        &Ctx {
            now: at,
            process: Some(key(PID)),
        },
    )
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

/// Adapt a fixture and reduce every input it yields.
fn fire(board: &mut Board, fixture: &str, event: &str, at: Instant) -> Vec<Effect> {
    adapt(fixture, event, at)
        .into_iter()
        .flat_map(|input| reduce::apply(board, input))
        .collect()
}

/// A board with one Codex agent that has sent `SessionStart{startup}`.
fn codex_board(t: &Clock) -> Board {
    let mut board = Board::default();
    let mut r = common::row(PID, Some("01a09c7a-d219-7ec3-9aa5-6a7aff30f201"));
    r.provider = Provider::Codex;
    reduce::apply_snapshot(
        &mut board,
        Snapshot {
            taken: t.at(0),
            kind: SnapshotKind::Processes(vec![r]),
        },
    );
    fire(
        &mut board,
        "session_start_startup.json",
        "SessionStart",
        t.at(0),
    );
    board
}

// ---------------------------------------------------------------------------
// The adapter
// ---------------------------------------------------------------------------

#[test]
fn should_key_a_codex_turn_by_turn_id_with_an_unknown_origin() {
    let e = only_event(adapt(
        "user_prompt_submit.json",
        "UserPromptSubmit",
        Instant::now(),
    ));
    assert_eq!(e.cause, Cause::Agent, "nothing says who typed it");
    assert_eq!(e.actor, Actor::Agent);
    match e.kind {
        EventKind::TurnStarted { turn, origin } => {
            assert_eq!(
                turn,
                Some(TurnKey::Turn("01a09c7c-4406-7740-bc28-cb15b82904d4".into()))
            );
            assert_eq!(origin, Origin::Unknown);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_hash_the_command_alone_so_a_request_joins_its_call() {
    let call = only_event(adapt(
        "pre_tool_use_bash.json",
        "PreToolUse",
        Instant::now(),
    ));
    let req = only_event(adapt(
        "permission_request_bash.json",
        "PermissionRequest",
        Instant::now(),
    ));
    let EventKind::ToolDispatched {
        call: id,
        tool,
        hash: call_hash,
    } = call.kind
    else {
        panic!("{:?}", call.kind);
    };
    assert_eq!(id.as_str(), "exec-0aa8ad62-4f36-4b21-b449-bf7806519cd2");
    assert_eq!(tool.as_str(), "Bash");
    assert_eq!(
        call_hash,
        input_hash(&json!("cargo test --offline")),
        "the command, not the whole input"
    );
    match req.kind {
        EventKind::PermissionAsked {
            call,
            hash,
            owner,
            kind,
            ..
        } => {
            assert_eq!(call, None, "0 of 31 captured requests carry an id");
            assert_eq!(hash, Some(call_hash), "the description is not hashed");
            assert_eq!(owner, Ownership::Unknown, "Codex does not say who reviews");
            assert_eq!(kind, GateKind::Approval);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_treat_request_user_input_as_a_human_question_with_its_id() {
    let e = only_event(adapt(
        "pre_tool_use_request_user_input.json",
        "PreToolUse",
        Instant::now(),
    ));
    assert!(matches!(
        e.kind,
        EventKind::PermissionAsked {
            call: Some(ref c),
            owner: Ownership::Human,
            kind: GateKind::Question,
            ..
        } if c.as_str() == "call-9"
    ));
}

#[test]
fn should_settle_a_tool_without_a_duration_and_name_a_subagent_actor() {
    let e = only_event(adapt(
        "post_tool_use_bash.json",
        "PostToolUse",
        Instant::now(),
    ));
    assert!(matches!(
        e.kind,
        EventKind::ToolSettled {
            outcome: ToolOutcome::Ok { ms: None },
            ..
        }
    ));
    let child = only_event(adapt(
        "pre_tool_use_subagent.json",
        "PreToolUse",
        Instant::now(),
    ));
    assert_eq!(
        child.actor,
        Actor::Subagent(SubagentId::from("01a09975-182f-78b1-bbc7-67bac5e83169"))
    );
}

#[test]
fn should_end_the_turn_on_stop_and_attribute_an_interrupt_to_the_user() {
    let stop = only_event(adapt("stop.json", "Stop", Instant::now()));
    assert_eq!(stop.cause, Cause::Agent);
    assert!(matches!(
        stop.kind,
        EventKind::TurnEnded {
            turn: Some(TurnKey::Turn(_)),
            outcome: TurnOutcome::Completed
        }
    ));
    let esc = only_event(adapt("interrupt.json", "Interrupt", Instant::now()));
    assert_eq!(esc.cause, Cause::User);
    assert!(matches!(
        esc.kind,
        EventKind::TurnEnded {
            turn: Some(TurnKey::Turn(_)),
            outcome: TurnOutcome::Interrupted { killed: None }
        }
    ));
}

#[test]
fn should_map_lifecycle_and_compaction_hooks_like_claude() {
    let start = only_event(adapt(
        "subagent_start.json",
        "SubagentStart",
        Instant::now(),
    ));
    assert_eq!(start.actor, Actor::Agent);
    assert!(matches!(
        start.kind,
        EventKind::SubagentStarted { ref subagent, .. }
            if subagent.as_str() == "01a09c73-bc71-7133-b068-e47fe6b91d33"
    ));
    let stop = only_event(adapt("subagent_stop.json", "SubagentStop", Instant::now()));
    assert!(matches!(stop.kind, EventKind::SubagentEnded { .. }));

    let compact = only_event(adapt(
        "pre_compact_auto_subagent.json",
        "PreCompact",
        Instant::now(),
    ));
    assert_eq!(compact.cause, Cause::Agent);
    assert!(matches!(compact.actor, Actor::Subagent(_)));
    assert!(matches!(
        compact.kind,
        EventKind::CompactionStarted {
            trigger: Trigger::Auto
        }
    ));

    let start = only_event(adapt(
        "session_start_compact.json",
        "SessionStart",
        Instant::now(),
    ));
    assert!(matches!(
        start.kind,
        EventKind::SessionStarted {
            source: StartSource::Compact,
            title: None
        }
    ));
    let end = only_event(adapt("session_end.json", "SessionEnd", Instant::now()));
    assert!(matches!(end.kind, EventKind::SessionEnded { ref reason } if &**reason == "other"));
}

#[test]
fn should_produce_nothing_for_hooks_codex_never_fires_or_that_lack_their_id() {
    let mut raw = fixture_json("codex/hooks/pre_tool_use_bash.json");
    raw.as_object_mut().unwrap().remove("tool_use_id");
    let line = scrub_for(Provider::Codex, raw, Some("PreToolUse"), PID).to_string();
    let Request::Hook(frame) = decode_request(&line).unwrap() else {
        panic!()
    };
    let ctx = Ctx {
        now: Instant::now(),
        process: None,
    };
    assert!(codex::hook(&frame, &ctx).is_empty());
    assert!(adapt("stop.json", "CwdChanged", Instant::now()).is_empty());
    assert!(adapt("stop.json", "Notification", Instant::now()).is_empty());
}

#[test]
fn should_hash_inputs_without_a_command_whole() {
    let patch = json!({"patch": "*** Begin Patch"});
    assert_eq!(codex::join_input(&patch), &patch);
    let cmd = json!({"command": "ls", "description": "list"});
    assert_eq!(codex::join_input(&cmd), &Value::String("ls".into()));
}

// ---------------------------------------------------------------------------
// Through the reducer
// ---------------------------------------------------------------------------

/// The captured order: `PreToolUse` 13-60 ms before `PermissionRequest`.
/// Nobody says who reviews it, so the chip goes grey, never red, and the
/// call settling clears it.
#[test]
fn should_show_an_unowned_codex_approval_grey_until_the_call_settles() {
    let t = Clock::new();
    let mut b = codex_board(&t);
    fire(
        &mut b,
        "user_prompt_submit.json",
        "UserPromptSubmit",
        t.at_ms(1000),
    );
    assert_eq!(truth(&b, PID), Phase::Working);
    fire(
        &mut b,
        "pre_tool_use_bash.json",
        "PreToolUse",
        t.at_ms(2000),
    );
    let fx = fire(
        &mut b,
        "permission_request_bash.json",
        "PermissionRequest",
        t.at_ms(2034),
    );
    assert!(publishes(&fx));
    assert!(
        agent(&b, PID).prompts.unjoined.is_empty(),
        "joined on the command hash"
    );
    assert_eq!(truth(&b, PID), Phase::Unknown);
    assert_eq!(
        shown(&b, PID).lamp(),
        Lamp::Grey,
        "never red without an owner"
    );

    fire(
        &mut b,
        "post_tool_use_bash.json",
        "PostToolUse",
        t.at_ms(6600),
    );
    assert_eq!(shown(&b, PID), Phase::Working);
}

#[test]
fn should_show_a_codex_question_red_at_once() {
    let t = Clock::new();
    let mut b = codex_board(&t);
    fire(
        &mut b,
        "user_prompt_submit.json",
        "UserPromptSubmit",
        t.at(1),
    );
    fire(
        &mut b,
        "pre_tool_use_request_user_input.json",
        "PreToolUse",
        t.at(2),
    );
    assert_eq!(shown(&b, PID), Phase::Attention);
}

#[test]
fn should_stop_at_once_on_interrupt_and_end_only_the_turn_a_stop_names() {
    let t = Clock::new();
    let mut b = codex_board(&t);
    fire(
        &mut b,
        "user_prompt_submit.json",
        "UserPromptSubmit",
        t.at(1),
    );
    fire(&mut b, "interrupt.json", "Interrupt", t.at(2));
    assert_eq!(shown(&b, PID), Phase::Stopped, "user-caused: no dwell");

    fire(
        &mut b,
        "user_prompt_submit.json",
        "UserPromptSubmit",
        t.at(3),
    );
    assert_eq!(truth(&b, PID), Phase::Working);
    // A Stop for some other turn is the late close of an earlier one.
    let mut other = fixture_json("codex/hooks/stop.json");
    other["turn_id"] = json!("01a09c7c-0000-7740-bc28-000000000000");
    let line = scrub_for(Provider::Codex, other, Some("Stop"), PID).to_string();
    let Request::Hook(frame) = decode_request(&line).unwrap() else {
        panic!()
    };
    let ctx = Ctx {
        now: t.at(4),
        process: Some(key(PID)),
    };
    for input in codex::hook(&frame, &ctx) {
        reduce::apply(&mut b, input);
    }
    assert_eq!(truth(&b, PID), Phase::Working, "a different turn's close");
    fire(&mut b, "stop.json", "Stop", t.at(5));
    assert_eq!(truth(&b, PID), Phase::Idle);
}

/// Codex fires `SessionStart{compact}` after every compaction (86 captured)
/// with the same thread id. It is not a rotation and must not reset a turn.
#[test]
fn should_keep_the_turn_across_a_compaction_session_start() {
    let t = Clock::new();
    let mut b = codex_board(&t);
    fire(
        &mut b,
        "user_prompt_submit.json",
        "UserPromptSubmit",
        t.at(1),
    );
    fire(&mut b, "pre_tool_use_bash.json", "PreToolUse", t.at(2));
    fire(
        &mut b,
        "session_start_compact.json",
        "SessionStart",
        t.at(3),
    );
    assert_eq!(truth(&b, PID), Phase::Working);
    assert_eq!(common::open_tools(&b, PID).len(), 1);
    assert_eq!(
        agent(&b, PID).session.as_deref(),
        Some("01a09c7a-d219-7ec3-9aa5-6a7aff30f201")
    );
}

/// A Codex agent has no transcript this crate reads, so discovery and
/// session adoption ask for no watcher.
#[test]
fn should_not_watch_a_transcript_for_a_codex_agent() {
    let t = Clock::new();
    let mut b = Board::default();
    let mut r = common::row(PID, None);
    r.provider = Provider::Codex;
    let fx = reduce::apply_snapshot(
        &mut b,
        Snapshot {
            taken: t.at(0),
            kind: SnapshotKind::Processes(vec![r]),
        },
    );
    assert!(
        !fx.iter()
            .any(|e| matches!(e, Effect::WatchTranscript { .. }))
    );
    let fx = fire(
        &mut b,
        "session_start_startup.json",
        "SessionStart",
        t.at(1),
    );
    assert!(
        !fx.iter()
            .any(|e| matches!(e, Effect::WatchTranscript { .. }))
    );
    assert!(
        agent(&b, PID).session.is_some(),
        "the session is still adopted"
    );
}
