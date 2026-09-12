//! T07 (golden, hidden) — the reducer, part I: routing, sessions, turns, tools.
//! The learner writes their own version of these in `t07_turns.rs`.

mod common;

use std::path::PathBuf;

use serde_json::json;

use common::*;
use switchboard::model::*;

// ---------------------------------------------------------------------------
// Routing and identity
// ---------------------------------------------------------------------------

#[test]
fn should_create_nothing_when_an_event_names_no_tracked_agent() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let fx = apply(&mut b, hook(999, "nobody", t.at(1), turn_started("p")));
    assert!(fx.is_empty());
    assert_eq!(b.agents.len(), 1);
    assert_eq!(truth(&b, 100), Phase::Idle);
}

#[test]
fn should_route_by_session_when_the_event_carries_no_process() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1"), (200, "s2")], t.at(0));
    apply(&mut b, unrouted(hook(0, "s2", t.at(1), turn_started("p"))));
    assert_eq!(truth(&b, 100), Phase::Idle);
    assert_eq!(truth(&b, 200), Phase::Working);
}

#[test]
fn should_route_by_session_when_the_process_is_not_tracked() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(555, "s1", t.at(1), turn_started("p")));
    assert_eq!(truth(&b, 100), Phase::Working);
}

#[test]
fn should_adopt_the_session_and_watch_its_transcript_when_the_agent_had_none() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, None)], t.at(0));
    let fx = apply(&mut b, hook(100, "late", t.at(1), turn_started("p")));
    assert_eq!(agent(&b, 100).session.as_deref(), Some("late"));
    assert!(fx.contains(&Effect::WatchTranscript {
        process: key(100),
        session: "late".into(),
        cwd: PathBuf::from("/home/u/Projects/p100"),
    }));
    assert_eq!(truth(&b, 100), Phase::Working);
}

#[test]
fn should_ignore_a_late_event_for_the_previous_session_after_clear() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "old")], t.at(0));
    apply(
        &mut b,
        by_user(
            100,
            "new",
            t.at(1),
            EventKind::SessionStarted {
                source: StartSource::Clear,
                title: None,
            },
        ),
    );
    let fx = apply(&mut b, hook(100, "old", t.at(2), turn_started("late")));
    assert!(fx.is_empty());
    assert_eq!(truth(&b, 100), Phase::Idle);
}

#[test]
fn should_reset_the_conversation_and_watch_the_new_transcript_when_the_session_rotates() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "old")], t.at(0));
    apply(&mut b, hook(100, "old", t.at(1), turn_started("p")));
    apply(&mut b, hook(100, "old", t.at(2), dispatched("c1", "Bash", &json!({}))));

    let fx = apply(
        &mut b,
        by_user(
            100,
            "new",
            t.at(3),
            EventKind::SessionStarted {
                source: StartSource::Clear,
                title: Some("fresh".into()),
            },
        ),
    );
    let a = agent(&b, 100);
    assert_eq!(a.session.as_deref(), Some("new"));
    assert_eq!(a.title.as_deref(), Some("fresh"));
    assert!(matches!(a.activity, Activity::Idle { after: None, .. }));
    assert!(fx.iter().any(|e| matches!(e, Effect::WatchTranscript { session, .. } if session.as_str() == "new")));
    assert_eq!(shown(&b, 100), Phase::Idle);
}

// ---------------------------------------------------------------------------
// Discovery and the first event
// ---------------------------------------------------------------------------

#[test]
fn should_show_unknown_for_a_discovered_agent_until_it_is_heard_from() {
    let t = Clock::new();
    let b = board_unobserved(&[(100, Some("s1"))], t.at(0));
    assert_eq!(truth(&b, 100), Phase::Unknown);
    assert_eq!(shown(&b, 100), Phase::Unknown);
    assert_eq!(shown(&b, 100).lamp(), Lamp::Grey);
}

#[test]
fn should_go_idle_on_session_start() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, Some("s1"))], t.at(0));
    let fx = apply(
        &mut b,
        hook(
            100,
            "s1",
            t.at(1),
            EventKind::SessionStarted {
                source: StartSource::Startup,
                title: None,
            },
        ),
    );
    assert_eq!(shown(&b, 100), Phase::Idle);
    assert!(publishes(&fx));
    assert_eq!(history(&fx).len(), 1);
}

// ---------------------------------------------------------------------------
// Turns
// ---------------------------------------------------------------------------

#[test]
fn should_show_working_at_once_and_record_it_when_a_turn_starts() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let fx = apply(&mut b, by_user(100, "s1", t.at(5), turn_started("p1")));
    assert_eq!(shown(&b, 100), Phase::Working);
    assert!(publishes(&fx));
    assert_eq!(
        history(&fx),
        [&HistoryEntry::Shown {
            process: key(100),
            session: Some("s1".into()),
            from: Phase::Idle,
            to: Phase::Working,
            held: std::time::Duration::from_secs(5),
        }]
    );
}

#[test]
fn should_publish_nothing_when_the_same_turn_starts_twice() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    let fx = apply(&mut b, inferred(100, "s1", t.at(2), turn_started("p1")));
    assert!(fx.is_empty(), "{fx:?}");
}

#[test]
fn should_keep_open_calls_when_a_new_prompt_starts_mid_turn() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), dispatched("c1", "Bash", &json!({}))));
    apply(&mut b, hook(100, "s1", t.at(3), turn_started("p2")));
    assert_eq!(open_tools(&b, 100), ["c1"]);
    let Activity::Turn { key, .. } = &agent(&b, 100).activity else {
        panic!()
    };
    assert_eq!(key, &Some(TurnKey::Prompt("p2".into())));
}

#[test]
fn should_go_idle_and_abandon_open_calls_when_the_turn_ends() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), dispatched("c1", "Bash", &json!({}))));
    apply(&mut b, hook(100, "s1", t.at(3), dispatched("c2", "Read", &json!({}))));
    let fx = apply(&mut b, hook(100, "s1", t.at(4), turn_ended(Some("p1"), TurnOutcome::Completed)));
    assert!(open_tools(&b, 100).is_empty());
    assert_eq!(truth(&b, 100), Phase::Idle);
    assert_eq!(shown(&b, 100), Phase::Idle, "de-escalation is immediate");
    assert!(publishes(&fx));
}

#[test]
fn should_ignore_a_turn_end_that_names_a_different_turn() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p2")));
    let fx = apply(&mut b, hook(100, "s1", t.at(2), turn_ended(Some("p1"), TurnOutcome::Completed)));
    assert!(fx.is_empty());
    assert_eq!(truth(&b, 100), Phase::Working);
}

#[test]
fn should_end_the_open_turn_when_the_end_carries_no_key() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p2")));
    apply(&mut b, hook(100, "s1", t.at(2), stop()));
    assert_eq!(truth(&b, 100), Phase::Idle);
}

#[test]
fn should_show_stopped_at_once_when_the_user_interrupts() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    // Working -> Stopped is an escalation, and the operator caused it.
    let fx = apply(&mut b, by_user(100, "s1", t.at(2), interrupted()));
    assert_eq!(truth(&b, 100), Phase::Stopped);
    assert_eq!(shown(&b, 100), Phase::Stopped);
    assert!(publishes(&fx));
}

#[test]
fn should_record_the_interrupt_when_its_line_lands_after_stop() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), stop()));
    assert_eq!(truth(&b, 100), Phase::Idle);
    apply(&mut b, by_user(100, "s1", t.at(3), interrupted()));
    assert_eq!(truth(&b, 100), Phase::Stopped);
}

#[test]
fn should_not_let_a_later_completion_erase_an_interrupt() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, by_user(100, "s1", t.at(2), interrupted()));
    let fx = apply(&mut b, hook(100, "s1", t.at(3), stop()));
    assert!(fx.is_empty());
    assert_eq!(truth(&b, 100), Phase::Stopped);
}

#[test]
fn should_clear_stopped_when_the_next_turn_starts() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, by_user(100, "s1", t.at(2), interrupted()));
    apply(&mut b, by_user(100, "s1", t.at(3), turn_started("p2")));
    assert_eq!(shown(&b, 100), Phase::Working);
}

#[test]
fn should_hold_the_screen_while_an_inferred_turn_failure_serves_its_dwell() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    apply(
        &mut b,
        inferred(
            100,
            "s1",
            t.at(2),
            turn_ended(
                None,
                TurnOutcome::Failed {
                    class: ErrorClass::RateLimit,
                },
            ),
        ),
    );
    assert_eq!(truth(&b, 100), Phase::Blocked);
    assert_eq!(shown(&b, 100), Phase::Working, "agent-caused and inferred: waits");
    assert!(agent(&b, 100).shown.pending.is_some());
}

#[test]
fn should_show_blocked_at_once_when_the_turn_failure_is_confirmed() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    apply(
        &mut b,
        hook(
            100,
            "s1",
            t.at(2),
            turn_ended(
                None,
                TurnOutcome::Failed {
                    class: ErrorClass::ServerError,
                },
            ),
        ),
    );
    assert_eq!(shown(&b, 100), Phase::Blocked);
}

#[test]
fn should_start_a_turn_when_a_tool_is_dispatched_while_idle() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({}))));
    assert_eq!(truth(&b, 100), Phase::Working);
    assert_eq!(open_tools(&b, 100), ["c1"]);
    let Activity::Turn { key, .. } = &agent(&b, 100).activity else {
        panic!()
    };
    assert_eq!(key, &None);
}

#[test]
fn should_leave_unobserved_for_idle_when_a_replayed_turn_ends() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, Some("s1"))], t.at(0));
    apply(&mut b, inferred(100, "s1", t.at(1), stop()));
    assert_eq!(truth(&b, 100), Phase::Idle);
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

#[test]
fn should_publish_nothing_when_the_same_call_is_dispatched_twice() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({"a": 1}))));
    let fx = apply(&mut b, inferred(100, "s1", t.at(2), dispatched("c1", "Bash", &json!({"a": 1}))));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(open_tools(&b, 100), ["c1"]);
}

#[test]
fn should_remove_only_the_settled_call_and_keep_working() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({}))));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c2", "Read", &json!({}))));
    apply(&mut b, hook(100, "s1", t.at(2), settled("c1")));
    assert_eq!(open_tools(&b, 100), ["c2"]);
    assert_eq!(truth(&b, 100), Phase::Working);
}

#[test]
fn should_publish_nothing_when_an_unknown_call_settles() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({}))));
    let fx = apply(&mut b, hook(100, "s1", t.at(2), settled("never-seen")));
    assert!(!publishes(&fx));
    assert_eq!(open_tools(&b, 100), ["c1"]);
}

#[test]
fn should_end_the_turn_as_interrupted_when_a_tool_fails_by_interrupt() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({}))));
    apply(
        &mut b,
        by_user(
            100,
            "s1",
            t.at(2),
            EventKind::ToolSettled {
                call: "c1".into(),
                outcome: ToolOutcome::Failed { interrupt: true },
            },
        ),
    );
    assert_eq!(shown(&b, 100), Phase::Stopped);
}

#[test]
fn should_keep_the_turn_when_a_tool_merely_fails() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({}))));
    apply(
        &mut b,
        hook(
            100,
            "s1",
            t.at(2),
            EventKind::ToolSettled {
                call: "c1".into(),
                outcome: ToolOutcome::Failed { interrupt: false },
            },
        ),
    );
    assert_eq!(truth(&b, 100), Phase::Working);
    assert!(open_tools(&b, 100).is_empty());
}

// ---------------------------------------------------------------------------
// Death, cwd, bookkeeping
// ---------------------------------------------------------------------------

#[test]
fn should_remove_the_agent_forget_it_and_record_it_when_it_dies() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1"), (200, "s2")], t.at(0));
    let fx = apply(&mut b, hook(100, "", t.at(1), EventKind::Died));
    assert!(!b.agents.contains_key(&key(100)));
    assert!(b.agents.contains_key(&key(200)));
    assert!(fx.contains(&Effect::Forget { process: key(100) }));
    assert!(fx.contains(&Effect::AppendHistory(HistoryEntry::AgentGone {
        process: key(100),
        session: Some("s1".into()),
    })));
    assert!(publishes(&fx));

    let again = apply(&mut b, hook(100, "", t.at(2), EventKind::Died));
    assert!(again.is_empty(), "a second death is a no-op");
}

#[test]
fn should_follow_cwd_changes_and_rename_the_workspace() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let to = PathBuf::from("/home/u/Projects/other");
    let fx = apply(&mut b, hook(100, "s1", t.at(1), EventKind::CwdChanged { to: to.clone() }));
    assert!(publishes(&fx));
    assert_eq!(agent(&b, 100).cwd, to);
    assert_eq!(agent(&b, 100).workspace.as_deref(), Some("other"));
    let again = apply(&mut b, hook(100, "s1", t.at(2), EventKind::CwdChanged { to }));
    assert!(again.is_empty());
}

#[test]
fn should_publish_nothing_for_session_end_because_it_is_not_liveness() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let fx = apply(
        &mut b,
        hook(100, "s1", t.at(1), EventKind::SessionEnded { reason: "clear".into() }),
    );
    assert!(fx.is_empty());
    assert!(b.agents.contains_key(&key(100)));
}

#[test]
fn should_stamp_last_event_only_when_something_changed() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(3), turn_started("p")));
    assert_eq!(agent(&b, 100).last_event, Some(t.at(3)));
    apply(&mut b, hook(100, "s1", t.at(9), turn_started("p")));
    assert_eq!(agent(&b, 100).last_event, Some(t.at(3)));
}
