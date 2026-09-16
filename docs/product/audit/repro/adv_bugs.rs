//! Adversarial audit: minimal reproductions. Every test here asserts the
//! behaviour the golden *should* have, and fails against it today.
//!
//! Run: `cargo test --test adv_bugs`

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};
use tokio::sync::watch;

use common::*;
use switchboard::daemon::rpc::resolve;
use switchboard::ingest::claude::{LineCtx, hook as hook_adapter, line};
use switchboard::ingest::frame::{Request, decode_request};
use switchboard::ingest::{Ctx, Input};
use switchboard::model::*;
use switchboard::proc::sweep;
use switchboard::reduce::{self, default_account, next_deadline, tick};
use switchboard::time::parse_rfc3339;
use switchboard::view::View;

fn feed(b: &mut Board, inputs: Vec<Input>) {
    for i in inputs {
        reduce::apply(b, i);
    }
}

fn tctx(pid: u32, session: &str, at: std::time::Instant, confidence: Confidence) -> LineCtx {
    LineCtx {
        process: key(pid),
        session: SessionId::from(session),
        account: default_account(),
        now: at,
        confidence,
    }
}

fn epoch(s: &str) -> u64 {
    parse_rfc3339(s)
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

// ---------------------------------------------------------------------------
// BUG 1 (High): a Rejected account limit never expires.
// ---------------------------------------------------------------------------

/// The shape of a real `isApiErrorMessage` line with `quotaLimits` (24 in the
/// local corpus). Replaying a months-old one from the 128 KiB discovery window
/// blocks *every* agent on the account, forever.
#[test]
fn should_not_block_every_agent_when_a_replayed_rate_limit_line_is_months_old() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1"), (200, "s2")], t.at(0));
    let old = json!({
        "type": "assistant",
        "isApiErrorMessage": true,
        "error": "rate_limit",
        "timestamp": "2026-03-01T00:00:00.000Z",
        "quotaLimits": {
            "status": "rejected",
            "resetsAt": epoch("2026-03-01T05:00:00Z"),
            "rateLimitType": "five_hour"
        },
        "message": {"role": "assistant", "content": [{"type": "text", "text": "limit"}]}
    });
    feed(&mut b, line(&old, &tctx(200, "s2", t.at(1), Confidence::Inferred)));
    tick(&mut b, t.at(20));
    tick(&mut b, t.at(24 * 3600));

    assert_ne!(
        truth(&b, 100),
        Phase::Blocked,
        "agent 100 never hit a limit; a reset six months ago still blocks it"
    );
}

/// Even a fresh rejection must lift once `resetsAt` passes. Nothing in the
/// reducer, `tick` or `next_deadline` ever looks at `resets_at`, and on this
/// machine the statusline is not wired to switchboard-hook, so no `allowed`
/// snapshot ever replaces it.
#[test]
fn should_clear_blocked_once_the_limit_resets_even_without_a_statusline() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    reduce::apply_snapshot(
        &mut b,
        Snapshot {
            taken: t.at(1),
            kind: SnapshotKind::Limits {
                account: default_account(),
                value: Limits {
                    status: LimitStatus::Rejected,
                    resets_at: Some(SystemTime::now() + Duration::from_secs(60)),
                    window: Some(LimitWindow::FiveHour),
                    used_percent: None,
                },
            },
        },
    );
    assert_eq!(truth(&b, 100), Phase::Blocked);

    // Two hours later the agent is working normally again.
    apply(&mut b, by_user(100, "s1", t.at(7200), turn_started("p2")));
    apply(&mut b, hook(100, "s1", t.at(7260), stop()));
    tick(&mut b, t.at(7300));
    assert_eq!(truth(&b, 100), Phase::Idle, "limit reset 2h ago");
}

// ---------------------------------------------------------------------------
// BUG 2 (Medium-High): Escape before the first assistant output is missed.
// ---------------------------------------------------------------------------

/// 24 of the corpus's plain `[Request interrupted by user]` lines that follow a
/// prompt directly carry no `interruptedMessageId` (no message was killed).
/// No hook fires either. The chip stays green `Working` for 10 minutes, then
/// goes `Idle` — never `Stopped`.
#[test]
fn should_stop_the_turn_when_escape_is_pressed_before_any_assistant_output() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let ctx = |s| tctx(100, "s1", t.at(s), Confidence::Confirmed);
    let prompt = json!({
        "type": "user", "promptId": "p1", "origin": {"kind": "human"},
        "message": {"role": "user", "content": "do the thing"}
    });
    let escape = json!({
        "type": "user", "promptId": "p1",
        "message": {"role": "user", "content": [{"type": "text", "text": "[Request interrupted by user]"}]}
    });
    feed(&mut b, line(&prompt, &ctx(1)));
    feed(&mut b, line(&escape, &ctx(2)));

    assert_eq!(truth(&b, 100), Phase::Stopped);
}

// ---------------------------------------------------------------------------
// BUG 3 (Medium): a human prompt with an image starts no turn.
// ---------------------------------------------------------------------------

/// 60 corpus prompts are `content: [text, image]` with `origin.kind: human`.
/// `user_line` only recognises string content, so after an interrupted turn
/// a successful image follow-up leaves the chip amber `Stopped`.
#[test]
fn should_start_a_turn_when_a_human_prompt_carries_an_image() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let ctx = |s| tctx(100, "s1", t.at(s), Confidence::Inferred);
    let interrupted = json!({
        "type": "user", "promptId": "p0", "interruptedMessageId": "msg_1",
        "message": {"role": "user", "content": [{"type": "text", "text": "[Request interrupted by user]"}]}
    });
    let image_prompt = json!({
        "type": "user", "promptId": "p1", "origin": {"kind": "human"},
        "message": {"role": "user", "content": [
            {"type": "text", "text": "what is this"},
            {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": "AA=="}}
        ]}
    });
    let answer = json!({
        "type": "assistant",
        "message": {"role": "assistant", "stop_reason": "end_turn",
                    "content": [{"type": "text", "text": "a cat"}]}
    });
    feed(&mut b, line(&interrupted, &ctx(1)));
    assert_eq!(truth(&b, 100), Phase::Stopped);
    feed(&mut b, line(&image_prompt, &ctx(2)));
    assert_eq!(truth(&b, 100), Phase::Working, "a human prompt started a turn");
    feed(&mut b, line(&answer, &ctx(3)));
    assert_eq!(truth(&b, 100), Phase::Idle, "the follow-up completed");
}

// ---------------------------------------------------------------------------
// BUG 4 (High): a nested `claude -p` hijacks the agent that spawned it.
// ---------------------------------------------------------------------------

/// 17 corpus Bash calls run `claude -p`. Print mode fires the user's hooks.
/// `resolve` walks past the untracked `claude -p` to the tracked agent, and
/// `SessionStarted` bypasses the `/clear` guard, so the agent's session
/// rotates to the nested one and every later hook from the real session is
/// dropped. Here the real agent's permission prompt is lost.
#[tokio::test]
async fn should_not_let_a_nested_print_mode_session_hijack_its_ancestor_agent() {
    let root = scratch_dir("adv_nested_claude");
    let fake = FakeProc { root: &root };
    let cwd = Path::new("/");
    fake.add(50, "claude", 1, 1000, &["claude"], cwd);
    fake.add(60, "bash", 50, 1001, &["bash", "-c", "claude -p hi"], cwd);
    fake.add(70, "claude", 60, 1002, &["claude", "-p", "hi"], cwd);
    fake.add(80, "sh", 70, 1003, &["sh"], cwd);
    let sessions = scratch_dir("adv_nested_claude_sessions");
    let swept: Vec<u32> = sweep(&root, &sessions).iter().map(|r| r.key.pid).collect();
    assert_eq!(swept, [50], "the print-mode child is correctly not an agent");

    let t = Clock::new();
    let mut b = board_with(&[(50, "real")], t.at(0));
    let (_tx, rx) = watch::channel(Arc::new(View::render(&b)));
    let root: Arc<Path> = Arc::from(root.as_path());

    // The nested session's SessionStart, forwarded from pid 80.
    let frame = r#"{"cmd":"hook","event":"SessionStart","pid":80,"session_id":"nested","source":"startup"}"#;
    let Ok(Request::Hook(frame)) = decode_request(frame) else {
        panic!()
    };
    let process = resolve(&root, frame.pid, &rx).await;
    assert_eq!(process, Some(key(50)), "attributed to the outer agent");
    feed(&mut b, hook_adapter(&frame, &Ctx { now: t.at(1), process }));

    // The real agent now asks for permission.
    let ask = r#"{"cmd":"hook","event":"PermissionRequest","pid":50,"session_id":"real","tool_name":"Bash","tool_input_hash":"0011223344556677"}"#;
    let Ok(Request::Hook(ask)) = decode_request(ask) else {
        panic!()
    };
    let process = resolve(&root, ask.pid, &rx).await;
    feed(&mut b, hook_adapter(&ask, &Ctx { now: t.at(2), process }));

    assert_eq!(
        agent(&b, 50).session.as_deref(),
        Some("real"),
        "session hijacked by the nested claude -p"
    );
    assert_eq!(truth(&b, 50), Phase::Attention, "real prompt dropped");
}

// ---------------------------------------------------------------------------
// BUG 5 (Medium): session drift never self-repairs from the sweep.
// ---------------------------------------------------------------------------

/// If the `SessionStart{clear}` hook is lost (forwarder 500 ms timeout, inbox
/// full, daemon restarting), the agent keeps the old session forever:
/// `discover` only fills a `None` session, so the sweep's fresh
/// `sessions/<pid>.json` value is ignored and every hook for the new
/// conversation is dropped by the `/clear` guard.
#[test]
fn should_adopt_the_session_the_sweep_reports_when_the_clear_hook_was_lost() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "old")], t.at(0));
    reduce::apply_snapshot(
        &mut b,
        Snapshot {
            taken: t.at(5),
            kind: SnapshotKind::Processes(vec![row(100, Some("new"))]),
        },
    );
    apply(&mut b, by_user(100, "new", t.at(6), turn_started("p1")));
    apply(&mut b, hook(100, "new", t.at(7), asked("Bash", &json!({"command": "ls"}))));

    assert_eq!(agent(&b, 100).session.as_deref(), Some("new"));
    assert_eq!(truth(&b, 100), Phase::Attention);
}

// ---------------------------------------------------------------------------
// BUG 6 (Medium): a subagent's gate is lifted by a *different* call settling.
// ---------------------------------------------------------------------------

/// On this machine SubagentStart is not registered, so a subagent prompt is
/// parked under `Actor::Subagent`. PostToolUse has no matcher, so the
/// subagent's parallel Read finishing lifts the Bash prompt still on screen.
/// Subagent transcripts are not watched, so nothing re-raises it.
#[test]
fn should_keep_a_subagent_prompt_while_a_parallel_call_of_that_subagent_settles() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(
        &mut b,
        from_subagent(100, "s1", "a1", t.at(2), asked("Bash", &json!({"command": "rm x"}))),
    );
    assert_eq!(truth(&b, 100), Phase::Attention);
    apply(&mut b, from_subagent(100, "s1", "a1", t.at(3), settled("toolu_read")));
    assert_eq!(truth(&b, 100), Phase::Attention, "the Bash prompt is still open");

    // Same with a row created by SubagentStart.
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(
        &mut b,
        hook(
            100,
            "s1",
            t.at(1),
            EventKind::SubagentStarted {
                subagent: SubagentId::from("a1"),
                kind: SubagentKind::Task,
            },
        ),
    );
    apply(
        &mut b,
        from_subagent(100, "s1", "a1", t.at(2), asked("Bash", &json!({"command": "rm x"}))),
    );
    apply(&mut b, from_subagent(100, "s1", "a1", t.at(3), settled("toolu_read")));
    assert_eq!(truth(&b, 100), Phase::Attention, "the Bash prompt is still open (row)");
}

// ---------------------------------------------------------------------------
// BUG 7 (Low-Medium): a duplicate settle lifts another call's parked prompt.
// ---------------------------------------------------------------------------

/// Every call settles twice: PostToolUse hook, then the transcript's
/// tool_result. The second finds no call and `lift_sole_unjoined` clears
/// whatever single prompt is parked — possibly another call's. Found by the
/// property run (`duplicate event changed state: ToolSettled`).
#[test]
fn should_keep_a_parked_prompt_when_a_duplicate_settle_arrives_for_another_call() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let ls = json!({"command": "ls"});
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), dispatched("A", "Read", &json!({"file_path": "/x"}))));
    apply(&mut b, hook(100, "s1", t.at(3), settled("A"))); // PostToolUse hook
    apply(&mut b, hook(100, "s1", t.at(4), asked("Bash", &ls))); // B's prompt, parked
    assert_eq!(truth(&b, 100), Phase::Attention);
    apply(&mut b, inferred(100, "s1", t.at(5), settled("A"))); // transcript tool_result A
    assert_eq!(truth(&b, 100), Phase::Attention, "B still waits for approval");
}

// ---------------------------------------------------------------------------
// BUG 8 (Low): dispatch consumes a parked prompt before noticing it is a dup.
// ---------------------------------------------------------------------------

/// `dispatch` calls `take_unjoined` and only then checks `tools.contains_key`,
/// so a repeated dispatch of an existing call eats a prompt meant for a
/// parallel identical call (same tool, same input).
#[test]
fn should_not_consume_a_parked_prompt_on_a_duplicate_dispatch() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let st = json!({"command": "git status"});
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), dispatched("A", "Bash", &st)));
    apply(&mut b, hook(100, "s1", t.at(3), asked("Bash", &st))); // joins A
    apply(&mut b, hook(100, "s1", t.at(4), asked("Bash", &st))); // B's prompt, parked
    assert_eq!(agent(&b, 100).prompts.unjoined.len(), 1);
    apply(&mut b, inferred(100, "s1", t.at(5), dispatched("A", "Bash", &st))); // transcript dup
    assert_eq!(agent(&b, 100).prompts.unjoined.len(), 1, "B's prompt was eaten");
}

// ---------------------------------------------------------------------------
// BUG 9 (Low): a repeated TurnStarted clears parked prompts before dedupe.
// ---------------------------------------------------------------------------

#[test]
fn should_keep_parked_prompts_when_the_same_turn_start_is_repeated() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1"))); // hook
    apply(&mut b, hook(100, "s1", t.at(2), asked("Bash", &json!({"command": "ls"}))));
    let fx = apply(&mut b, inferred(100, "s1", t.at(3), turn_started("p1"))); // transcript dup
    assert_eq!(truth(&b, 100), Phase::Attention);
    assert!(fx.is_empty(), "a duplicate is a no-op: {fx:?}");
}

// ---------------------------------------------------------------------------
// BUG 10 (Low): duplicates that are not no-ops (spec: event.rs:3-6).
// ---------------------------------------------------------------------------

#[test]
fn should_publish_nothing_for_a_duplicate_session_start_or_compaction_start() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let start = EventKind::SessionStarted {
        source: StartSource::Startup,
        title: None,
    };
    let fx = apply(&mut b, hook(100, "s1", t.at(1), start));
    assert!(fx.is_empty(), "second SessionStart{{startup}}: {fx:?}");
    let compact = || EventKind::CompactionStarted {
        trigger: Trigger::Auto,
    };
    apply(&mut b, hook(100, "s1", t.at(2), compact()));
    let fx = apply(&mut b, hook(100, "s1", t.at(3), compact()));
    assert!(fx.is_empty(), "second CompactionStarted: {fx:?}");
}

// ---------------------------------------------------------------------------
// BUG 11 (Low): an agent discovered under a Rejected limit is never settled.
// ---------------------------------------------------------------------------

#[test]
fn should_show_blocked_for_an_agent_discovered_while_the_account_is_rejected() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    reduce::apply_snapshot(&mut b, limits_at(LimitStatus::Rejected, t.at(1)));
    tick(&mut b, t.at(20));
    reduce::apply_snapshot(
        &mut b,
        Snapshot {
            taken: t.at(30),
            kind: SnapshotKind::Processes(vec![row(100, Some("s1")), row(200, None)]),
        },
    );
    // The core only ticks at a deadline, and there is none to wake it.
    assert_eq!(next_deadline(&b), None);
    let a = agent(&b, 200);
    assert_eq!(truth(&b, 200), Phase::Blocked);
    assert!(
        a.shown.phase == Phase::Blocked || a.shown.pending.is_some(),
        "shown {:?}, no pending escalation, and no deadline to ever fix it",
        a.shown.phase
    );
}

// ---------------------------------------------------------------------------
// BUG 12 (Low): an unrelated Confirmed hook skips an Inferred dwell.
// ---------------------------------------------------------------------------

/// A replayed `Failed` turn is `Inferred` and waits 10 s. Any Confirmed hook
/// for the agent in that window — even `SessionEnd`, which changes nothing —
/// is passed to `settle` with *its* confidence and promotes it at once.
#[test]
fn should_not_skip_an_inferred_dwell_because_an_unrelated_hook_is_confirmed() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(
        &mut b,
        inferred(
            100,
            "s1",
            t.at(2),
            turn_ended(None, TurnOutcome::Failed { class: ErrorClass::ServerError }),
        ),
    );
    assert_eq!(shown(&b, 100), Phase::Working, "dwelling");
    apply(
        &mut b,
        hook(100, "s1", t.at(3), EventKind::SessionEnded { reason: "other".into() }),
    );
    assert_eq!(shown(&b, 100), Phase::Working, "still inside the 10 s dwell");
}

// ---------------------------------------------------------------------------
// BUG 13 (Low): compaction expiry hides a stale turn for one extra wakeup.
// ---------------------------------------------------------------------------

#[test]
fn should_leave_no_past_deadline_after_a_single_tick() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(
        &mut b,
        hook(100, "s1", t.at(1), EventKind::CompactionStarted { trigger: Trigger::Auto }),
    );
    let now = t.at(1 + 601);
    tick(&mut b, now);
    assert!(
        next_deadline(&b).is_none_or(|d| d > now),
        "stale-turn deadline is already due right after the tick"
    );
}

// ---------------------------------------------------------------------------
// BUG 14 (Low): Died for an untracked process falls back to session routing.
// ---------------------------------------------------------------------------

/// The scanner sends `Died` with `session: ""`. `route` falls back to a
/// session search when the process is not tracked, so an agent whose session
/// was set to `""` (the frame decoder accepts `"session_id":""`) is removed
/// by some other process's death.
#[test]
fn should_not_remove_an_agent_when_an_untracked_process_dies() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, None)], t.at(0));
    apply(&mut b, hook(100, "", t.at(1), turn_started("p1")));
    let died = Event {
        at: Moment { local: t.at(2), claimed: None },
        process: Some(ProcKey { pid: 999, start: 1 }),
        session: SessionId::from(""),
        actor: Actor::Agent,
        cause: Cause::Agent,
        confidence: Confidence::Confirmed,
        kind: EventKind::Died,
    };
    apply(&mut b, died);
    assert!(b.agents.contains_key(&key(100)), "agent 100 was killed by pid 999's death");
    let _: Option<Value> = None;
}
