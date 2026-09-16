//! Behaviours pinned by the September 2026 signal capture.
//!
//! Every payload here is the shape recorded in
//! `~/.local/state/switchboard/hookcap` from Claude Code 2.1.269/2.1.270,
//! reduced to the fields the adapters read, and each test runs the real
//! adapter into the real reducer. The unit-level rules live in `t03`, `t08`
//! and `t09`; this file keeps the four end-to-end stories that first found
//! them wrong (`tools/hookcap/FINDINGS-2026-09-15.md`).

mod common;

use std::time::Instant;

use serde_json::{Value, json};

use common::{Clock, board_with, fixture_json, key, truth};
use switchboard::ingest::claude;
use switchboard::ingest::frame::{HookFrame, StatuslineFrame};
use switchboard::ingest::{Ctx, Input};
use switchboard::model::{Board, LimitStatus, LimitWindow, Limits, Phase, SnapshotKind};
use switchboard::reduce;

const PID: u32 = 100;
const SESSION: &str = "s1";

fn ctx(at: Instant) -> Ctx {
    Ctx {
        now: at,
        process: Some(key(PID)),
    }
}

/// Adapt a hook payload the way the daemon does, then reduce every input.
fn fire(board: &mut Board, event: &str, at: Instant, payload: Value) {
    let frame = HookFrame {
        event: event.into(),
        pid: Some(PID),
        session: SESSION.into(),
        payload,
    };
    for input in claude::hook(&frame, &ctx(at)) {
        reduce::apply(board, input);
    }
}

/// The limits a status-line fixture yields, if any.
fn limits_from(fixture: &str, at: Instant) -> Option<Limits> {
    let frame = StatuslineFrame {
        pid: Some(PID),
        session: SESSION.into(),
        payload: fixture_json(&format!("claude/statusline/{fixture}")),
    };
    claude::statusline(&frame, &reduce::default_account(), &ctx(at))
        .into_iter()
        .find_map(|i| match i {
            Input::Snapshot(s) => match s.kind {
                SnapshotKind::Limits { value, .. } => Some(value),
                _ => None,
            },
            Input::Event(_) => None,
        })
}

/// An `AskUserQuestion` as 2.1.269 actually delivers it: `PreToolUse` with the
/// call id, then `PermissionRequest` 16-214 ms later carrying the same
/// `tool_input` and no id at all, then `PostToolUse` when the human answers.
#[test]
fn should_leave_attention_when_an_answered_question_was_announced_by_two_hooks() {
    let t = Clock::new();
    let mut board = board_with(&[(PID, SESSION)], t.at(0));
    let questions = json!({"questions": [{"question": "which?", "header": "pick",
        "options": [{"label": "a", "description": "first"}], "multiSelect": false}]});

    fire(
        &mut board,
        "UserPromptSubmit",
        t.at_ms(0),
        json!({"session_id": SESSION, "prompt_id": "p1"}),
    );
    fire(
        &mut board,
        "PreToolUse",
        t.at_ms(1000),
        json!({"session_id": SESSION,
        "tool_name": "AskUserQuestion", "tool_use_id": "toolu_1", "tool_input": questions}),
    );
    fire(
        &mut board,
        "PermissionRequest",
        t.at_ms(1078),
        json!({"session_id": SESSION,
        "tool_name": "AskUserQuestion", "tool_input": questions}),
    );
    assert_eq!(truth(&board, PID), Phase::Attention, "the question is open");

    // The operator answers: PostToolUse carries the id from PreToolUse.
    fire(
        &mut board,
        "PostToolUse",
        t.at_ms(91000),
        json!({"session_id": SESSION,
        "tool_name": "AskUserQuestion", "tool_use_id": "toolu_1", "duration_ms": 90000}),
    );

    assert_eq!(
        truth(&board, PID),
        Phase::Working,
        "answered: nothing parked keeps it red"
    );
}

/// 47 of 56 captured `StopFailure` payloads carry `agent_id`: the failing turn
/// was a subagent's. The parent was still working.
#[test]
fn should_keep_the_agent_working_when_a_subagents_turn_fails() {
    let t = Clock::new();
    let mut board = board_with(&[(PID, SESSION)], t.at(0));

    fire(
        &mut board,
        "UserPromptSubmit",
        t.at(1),
        json!({"session_id": SESSION, "prompt_id": "p1"}),
    );
    fire(
        &mut board,
        "SubagentStart",
        t.at(2),
        json!({"session_id": SESSION,
        "agent_id": "child-1", "agent_type": "general-purpose"}),
    );
    assert_eq!(truth(&board, PID), Phase::Working);

    fire(
        &mut board,
        "StopFailure",
        t.at(3),
        json!({"session_id": SESSION, "prompt_id": "p1",
        "agent_id": "child-1", "agent_type": "general-purpose", "error": "rate_limit"}),
    );

    assert_eq!(
        truth(&board, PID),
        Phase::Working,
        "a child's failure is the child's"
    );

    // The agent's own failure still counts.
    fire(
        &mut board,
        "StopFailure",
        t.at(4),
        json!({"session_id": SESSION, "prompt_id": "p1",
        "error": "rate_limit"}),
    );
    assert_eq!(truth(&board, PID), Phase::Blocked);
}

/// The 2.1.269 status line reports limits per window: 543 of 552 captured
/// payloads use this shape and none carries `status`, `resetsAt` or
/// `utilization`. The fullest window decides.
#[test]
fn should_read_rate_limits_from_the_per_window_status_line_shape() {
    let t = Clock::new();
    let limits = limits_from("rate_limits_windows.json", t.at(0))
        .expect("a Limits snapshot from the per-window shape");
    assert_eq!(limits.status, LimitStatus::Allowed);
    assert_eq!(limits.used_percent, Some(61), "the worst window decides");
    assert_eq!(limits.window, Some(LimitWindow::SevenDay));
    assert!(
        limits.resets_at.is_some(),
        "a reset time is needed to expire a block"
    );

    let only_seven = limits_from("rate_limits_seven_day_only.json", t.at(0)).expect("one window");
    assert_eq!(only_seven.used_percent, Some(62));

    assert_eq!(
        limits_from("rate_limits_empty.json", t.at(0)),
        None,
        "an empty object says nothing about limits"
    );
}

/// 54 captured Claude subagents started and never reported a stop. Their
/// rows expire, so the agent stops claiming to delegate.
#[test]
fn should_stop_claiming_delegation_when_a_subagent_row_goes_stale() {
    let t = Clock::new();
    let mut board = board_with(&[(PID, SESSION)], t.at(0));

    fire(
        &mut board,
        "UserPromptSubmit",
        t.at(1),
        json!({"session_id": SESSION, "prompt_id": "p1"}),
    );
    fire(
        &mut board,
        "SubagentStart",
        t.at(2),
        json!({"session_id": SESSION,
        "agent_id": "child-1", "agent_type": "general-purpose"}),
    );
    fire(
        &mut board,
        "Stop",
        t.at(3),
        json!({"session_id": SESSION, "prompt_id": "p1"}),
    );
    assert_eq!(
        truth(&board, PID),
        Phase::Delegating,
        "the child is believed to be running"
    );

    // Six hours later, with no SubagentStop ever delivered.
    reduce::tick(&mut board, t.at(6 * 3600));

    assert_eq!(truth(&board, PID), Phase::Idle);
}
