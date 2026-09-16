//! T07 — YOUR tests for the reducer: routing, sessions, turns, tools.
//!
//! Work one group at a time. Write every body in the group, run it and see it
//! fail, then implement until the group passes. `tests/common/mod.rs` has the
//! builders. Read it first.
//!
//! The list is not complete. Section "yours" at the bottom wants at least four
//! more tests. Good places to look: what `last_event` should do, what a
//! transcript replay can do to an agent nobody has heard from, and what order
//! an interrupt and a `Stop` can arrive in.

mod common;

use std::path::PathBuf;
use std::time::Duration;

use serde_json::json;

use common::*;
use switchboard::model::*;

// ---------------------------------------------------------------------------
// Routing and identity
// ---------------------------------------------------------------------------

/// An event whose pid and session match nothing produces no effects and
/// creates no agent. Agents exist because processes do.
#[test]
fn should_create_nothing_when_an_event_names_no_tracked_agent() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let fx = apply(&mut b, hook(999, "nope", t.at(1), turn_started("p1")));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(b.agents.len(), 1);
    assert_eq!(shown(&b, 100), Phase::Idle);
}

/// `process: None` routes by session. With two agents, only the one owning
/// that session moves.
#[test]
fn should_route_by_session_when_the_event_carries_no_process() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1"), (200, "s2")], t.at(0));
    let fx = apply(&mut b, unrouted(hook(100, "s2", t.at(1), turn_started("p1"))));
    assert!(publishes(&fx));
    assert_eq!(shown(&b, 200), Phase::Working);
    assert_eq!(shown(&b, 100), Phase::Idle);
}

/// A pid the board doesn't track falls back to the session too.
#[test]
fn should_route_by_session_when_the_process_is_not_tracked() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1"), (200, "s2")], t.at(0));
    let fx = apply(&mut b, hook(999, "s2", t.at(1), turn_started("p1")));
    assert!(publishes(&fx));
    assert_eq!(shown(&b, 200), Phase::Working);
    assert_eq!(shown(&b, 100), Phase::Idle);
    assert_eq!(b.agents.len(), 2, "no agent for pid 999");
}

/// An agent discovered without a session adopts the first event's session and
/// asks for its transcript to be watched. Check the exact `WatchTranscript`
/// effect, cwd included.
#[test]
fn should_adopt_the_session_and_watch_its_transcript_when_the_agent_had_none() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, None)], t.at(0));
    let fx = apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
    assert_eq!(agent(&b, 100).session.as_deref(), Some("s1"));
    assert!(fx.contains(&Effect::WatchTranscript {
        process: key(100),
        session: SessionId::from("s1"),
        cwd: PathBuf::from("/home/u/Projects/p100"),
    }));
    assert!(publishes(&fx));
    assert_eq!(shown(&b, 100), Phase::Working);
}

/// After `SessionStarted{Clear}` moves the agent to a new session, an event
/// still carrying the old session is a no-op: no effects, no change.
#[test]
fn should_ignore_a_late_event_for_the_previous_session_after_clear() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(
        &mut b,
        by_user(
            100,
            "s2",
            t.at(1),
            EventKind::SessionStarted {
                source: StartSource::Clear,
                title: None,
            },
        ),
    );
    assert_eq!(agent(&b, 100).session.as_deref(), Some("s2"));
    let fx = apply(&mut b, hook(100, "s1", t.at(2), turn_started("p1")));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(shown(&b, 100), Phase::Idle);
    assert_eq!(agent(&b, 100).session.as_deref(), Some("s2"));
}

/// A rotation mid-turn wipes the old conversation. The agent is idle with no
/// outcome, the title updates, the new transcript is watched, and the chip
/// shows idle.
#[test]
fn should_reset_the_conversation_and_watch_the_new_transcript_when_the_session_rotates() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), dispatched("c1", "Bash", &json!({"command": "ls"}))));
    assert_eq!(shown(&b, 100), Phase::Working);
    let fx = apply(
        &mut b,
        by_user(
            100,
            "s2",
            t.at(3),
            EventKind::SessionStarted {
                source: StartSource::Clear,
                title: Some("fresh".into()),
            },
        ),
    );
    let a = agent(&b, 100);
    assert!(matches!(a.activity, Activity::Idle { after: None, .. }), "{:?}", a.activity);
    assert_eq!(a.session.as_deref(), Some("s2"));
    assert_eq!(a.title.as_deref(), Some("fresh"));
    assert!(open_tools(&b, 100).is_empty());
    assert!(fx.contains(&Effect::WatchTranscript {
        process: key(100),
        session: SessionId::from("s2"),
        cwd: PathBuf::from("/home/u/Projects/p100"),
    }));
    assert!(publishes(&fx));
    assert_eq!(shown(&b, 100), Phase::Idle);
}

// ---------------------------------------------------------------------------
// Discovery and the first event
// ---------------------------------------------------------------------------

/// Discovered and never heard from: truth and shown are both `Unknown`, which
/// paints grey.
#[test]
fn should_show_unknown_for_a_discovered_agent_until_it_is_heard_from() {
    let t = Clock::new();
    let b = board_unobserved(&[(100, Some("s1"))], t.at(0));
    assert_eq!(truth(&b, 100), Phase::Unknown);
    assert_eq!(shown(&b, 100), Phase::Unknown);
}

/// `SessionStarted{Startup}` on an unobserved agent shows `Idle`, publishes,
/// and records exactly one history entry.
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
    let h = history(&fx);
    assert_eq!(h.len(), 1);
    assert!(matches!(
        h[0],
        HistoryEntry::Shown {
            from: Phase::Unknown,
            to: Phase::Idle,
            ..
        }
    ));
}

// ---------------------------------------------------------------------------
// Turns
// ---------------------------------------------------------------------------

/// A turn starting shows `Working` immediately. The history entry is exact:
/// from `Idle`, to `Working`, `held` equal to the time since the last change.
#[test]
fn should_show_working_at_once_and_record_it_when_a_turn_starts() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let fx = apply(&mut b, by_user(100, "s1", t.at(5), turn_started("p1")));
    assert_eq!(shown(&b, 100), Phase::Working);
    assert_eq!(truth(&b, 100), Phase::Working);
    assert!(publishes(&fx));
    assert_eq!(
        history(&fx),
        [&HistoryEntry::Shown {
            process: key(100),
            session: Some(SessionId::from("s1")),
            from: Phase::Idle,
            to: Phase::Working,
            held: Duration::from_secs(5),
        }]
    );
}

/// The hook and the transcript both report the same prompt. The second report
/// produces no effects at all.
#[test]
fn should_publish_nothing_when_the_same_turn_starts_twice() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    let fx = apply(&mut b, inferred(100, "s1", t.at(2), turn_started("p1")));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(shown(&b, 100), Phase::Working);
}

/// A different prompt key mid-turn keeps the open calls and takes the new key.
#[test]
fn should_keep_open_calls_when_a_new_prompt_starts_mid_turn() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), dispatched("c1", "Bash", &json!({"command": "ls"}))));
    apply(&mut b, by_user(100, "s1", t.at(3), turn_started("p2")));
    assert_eq!(open_tools(&b, 100), ["c1"]);
    let Activity::Turn { key, .. } = &agent(&b, 100).activity else {
        panic!("not in a turn");
    };
    assert_eq!(*key, Some(TurnKey::Prompt(PromptId::from("p2"))));
    assert_eq!(shown(&b, 100), Phase::Working);
}

/// `TurnEnded` abandons every open call, goes idle, and shows it immediately
/// (de-escalation never waits).
#[test]
fn should_go_idle_and_abandon_open_calls_when_the_turn_ends() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), dispatched("c1", "Bash", &json!({"command": "ls"}))));
    apply(&mut b, hook(100, "s1", t.at(2), dispatched("c2", "Read", &json!({"file_path": "x"}))));
    let fx = apply(&mut b, hook(100, "s1", t.at(3), stop()));
    assert_eq!(shown(&b, 100), Phase::Idle);
    assert!(open_tools(&b, 100).is_empty());
    assert!(publishes(&fx));
    assert!(matches!(
        history(&fx)[..],
        [HistoryEntry::Shown {
            from: Phase::Working,
            to: Phase::Idle,
            ..
        }]
    ));
}

/// A `TurnEnded` whose key names a different turn than the open one is a
/// no-op.
#[test]
fn should_ignore_a_turn_end_that_names_a_different_turn() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    let fx = apply(
        &mut b,
        hook(100, "s1", t.at(2), turn_ended(Some("p0"), TurnOutcome::Completed)),
    );
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(shown(&b, 100), Phase::Working);
}

/// Escape: `Working` → `Stopped` is an escalation, and it shows immediately
/// because the user caused it.
#[test]
fn should_show_stopped_at_once_when_the_user_interrupts() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    let fx = apply(&mut b, by_user(100, "s1", t.at(2), interrupted()));
    assert_eq!(truth(&b, 100), Phase::Stopped);
    assert_eq!(shown(&b, 100), Phase::Stopped);
    assert!(matches!(
        history(&fx)[..],
        [HistoryEntry::Shown {
            from: Phase::Working,
            to: Phase::Stopped,
            ..
        }]
    ));
}

/// The next prompt after an interrupt clears `Stopped`.
#[test]
fn should_clear_stopped_when_the_next_turn_starts() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, by_user(100, "s1", t.at(2), interrupted()));
    assert_eq!(shown(&b, 100), Phase::Stopped);
    apply(&mut b, by_user(100, "s1", t.at(3), turn_started("p2")));
    assert_eq!(shown(&b, 100), Phase::Working);
    apply(&mut b, hook(100, "s1", t.at(4), stop()));
    assert_eq!(shown(&b, 100), Phase::Idle, "the interrupt does not outlive the next turn");
}

/// An agent-caused, *inferred* turn failure: truth is `Blocked`, the screen
/// still shows `Working`, and an escalation is pending.
#[test]
fn should_hold_the_screen_while_an_inferred_turn_failure_serves_its_dwell() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    let failed = turn_ended(
        None,
        TurnOutcome::Failed {
            class: ErrorClass::RateLimit,
        },
    );
    let fx = apply(&mut b, inferred(100, "s1", t.at(2), failed));
    assert_eq!(truth(&b, 100), Phase::Blocked);
    assert_eq!(shown(&b, 100), Phase::Working);
    let pending = agent(&b, 100).shown.pending.as_ref().expect("escalation pending");
    assert_eq!(pending.to, Phase::Blocked);
    assert_eq!(pending.promote_at, t.at(2) + Shown::INFERRED_DWELL);
    assert!(history(&fx).is_empty(), "nothing shown yet");
}

/// The same failure, *confirmed*, shows `Blocked` immediately.
#[test]
fn should_show_blocked_at_once_when_the_turn_failure_is_confirmed() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    let failed = turn_ended(
        None,
        TurnOutcome::Failed {
            class: ErrorClass::ServerError,
        },
    );
    let fx = apply(&mut b, hook(100, "s1", t.at(2), failed));
    assert_eq!(shown(&b, 100), Phase::Blocked);
    assert!(agent(&b, 100).shown.pending.is_none());
    assert!(publishes(&fx));
}

/// A dispatch while idle starts a turn with no key.
#[test]
fn should_start_a_turn_when_a_tool_is_dispatched_while_idle() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let fx = apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({"command": "ls"}))));
    assert_eq!(shown(&b, 100), Phase::Working);
    assert!(publishes(&fx));
    assert_eq!(open_tools(&b, 100), ["c1"]);
    assert!(matches!(agent(&b, 100).activity, Activity::Turn { key: None, .. }));
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// The same call dispatched twice (hook, then transcript) produces no effects.
#[test]
fn should_publish_nothing_when_the_same_call_is_dispatched_twice() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let call = dispatched("c1", "Bash", &json!({"command": "ls"}));
    apply(&mut b, hook(100, "s1", t.at(1), call.clone()));
    let fx = apply(&mut b, inferred(100, "s1", t.at(2), call));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(open_tools(&b, 100), ["c1"]);
}

/// Two calls open, one settles: the other stays, and the agent is still
/// working.
#[test]
fn should_remove_only_the_settled_call_and_keep_working() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({"command": "ls"}))));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c2", "Read", &json!({"file_path": "x"}))));
    let fx = apply(&mut b, hook(100, "s1", t.at(2), settled("c1")));
    assert_eq!(open_tools(&b, 100), ["c2"]);
    assert_eq!(shown(&b, 100), Phase::Working);
    assert!(publishes(&fx));
    assert!(history(&fx).is_empty(), "still working, nothing to record");
}

/// A settle for a call never dispatched (with no prompts parked) publishes
/// nothing.
#[test]
fn should_publish_nothing_when_an_unknown_call_settles() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    let fx = apply(&mut b, hook(100, "s1", t.at(2), settled("never")));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(shown(&b, 100), Phase::Working);
}

/// `ToolSettled{Failed{interrupt: true}}` from the user shows `Stopped`.
#[test]
fn should_end_the_turn_as_interrupted_when_a_tool_fails_by_interrupt() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({"command": "sleep 99"}))));
    let failed = EventKind::ToolSettled {
        call: ToolUseId::from("c1"),
        outcome: ToolOutcome::Failed { interrupt: true },
    };
    let fx = apply(&mut b, by_user(100, "s1", t.at(2), failed));
    assert_eq!(shown(&b, 100), Phase::Stopped);
    assert!(open_tools(&b, 100).is_empty());
    assert!(publishes(&fx));
}

/// A plain failure removes the call but keeps the turn.
#[test]
fn should_keep_the_turn_when_a_tool_merely_fails() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({"command": "false"}))));
    apply(&mut b, hook(100, "s1", t.at(1), dispatched("c2", "Read", &json!({"file_path": "x"}))));
    let failed = EventKind::ToolSettled {
        call: ToolUseId::from("c1"),
        outcome: ToolOutcome::Failed { interrupt: false },
    };
    apply(&mut b, hook(100, "s1", t.at(2), failed));
    assert_eq!(open_tools(&b, 100), ["c2"]);
    assert_eq!(shown(&b, 100), Phase::Working);
}

// ---------------------------------------------------------------------------
// Death, cwd, bookkeeping
// ---------------------------------------------------------------------------

/// `Died` removes only that agent, emits `Forget` and `AgentGone`, and
/// publishes. A second `Died` for the same process is a no-op.
#[test]
fn should_remove_the_agent_forget_it_and_record_it_when_it_dies() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1"), (200, "s2")], t.at(0));
    let fx = apply(&mut b, hook(100, "s1", t.at(1), EventKind::Died));
    assert!(!b.agents.contains_key(&key(100)));
    assert!(b.agents.contains_key(&key(200)));
    assert!(fx.contains(&Effect::Forget { process: key(100) }));
    assert_eq!(
        history(&fx),
        [&HistoryEntry::AgentGone {
            process: key(100),
            session: Some(SessionId::from("s1")),
        }]
    );
    assert!(publishes(&fx));
    let again = apply(&mut b, hook(100, "s1", t.at(2), EventKind::Died));
    assert!(again.is_empty(), "{again:?}");
}

/// `CwdChanged` updates `cwd` and `workspace`. The same path again is a no-op.
#[test]
fn should_follow_cwd_changes_and_rename_the_workspace() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let moved = EventKind::CwdChanged {
        to: PathBuf::from("/home/u/Projects/other"),
    };
    let fx = apply(&mut b, hook(100, "s1", t.at(1), moved.clone()));
    assert_eq!(agent(&b, 100).cwd, PathBuf::from("/home/u/Projects/other"));
    assert_eq!(agent(&b, 100).workspace.as_deref(), Some("other"));
    assert!(publishes(&fx));
    let again = apply(&mut b, hook(100, "s1", t.at(2), moved));
    assert!(again.is_empty(), "{again:?}");
}

/// `SessionEnded` is not liveness. The agent stays and nothing is published.
#[test]
fn should_publish_nothing_for_session_end_because_it_is_not_liveness() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let fx = apply(
        &mut b,
        hook(
            100,
            "s1",
            t.at(1),
            EventKind::SessionEnded {
                reason: "exit".into(),
            },
        ),
    );
    assert!(fx.is_empty(), "{fx:?}");
    assert!(b.agents.contains_key(&key(100)));
    assert_eq!(shown(&b, 100), Phase::Idle);
}

// ---------------------------------------------------------------------------
// yours: at least four more
// ---------------------------------------------------------------------------

/// `last_event` moves with a real change and stays put on a duplicate.
#[test]
fn should_stamp_last_event_only_when_something_changed() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    assert_eq!(agent(&b, 100).last_event, Some(t.at(1)));
    apply(&mut b, inferred(100, "s1", t.at(2), turn_started("p1")));
    assert_eq!(agent(&b, 100).last_event, Some(t.at(1)), "a duplicate is not an event");
    apply(&mut b, hook(100, "s1", t.at(3), stop()));
    assert_eq!(agent(&b, 100).last_event, Some(t.at(3)));
}

/// The hook's `Stop` and the transcript's interrupt line can land in either
/// order. Interrupt after Stop still records `Stopped`.
#[test]
fn should_record_stopped_when_an_interrupt_lands_after_stop() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, hook(100, "s1", t.at(2), stop()));
    assert_eq!(shown(&b, 100), Phase::Idle);
    let fx = apply(&mut b, by_user(100, "s1", t.at(3), interrupted()));
    assert_eq!(shown(&b, 100), Phase::Stopped);
    assert!(publishes(&fx));
}

/// ... and a `Completed` after the interrupt never erases it.
#[test]
fn should_keep_stopped_when_a_completed_end_lands_after_an_interrupt() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    apply(&mut b, by_user(100, "s1", t.at(1), turn_started("p1")));
    apply(&mut b, by_user(100, "s1", t.at(2), interrupted()));
    let fx = apply(&mut b, hook(100, "s1", t.at(3), stop()));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(shown(&b, 100), Phase::Stopped);
}

/// A transcript replay is the first thing an unheard agent says. A replayed
/// dispatch takes it straight from grey to working: green is calm, so no
/// dwell applies.
#[test]
fn should_start_working_when_a_replayed_dispatch_reaches_an_unheard_agent() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, Some("s1"))], t.at(0));
    let fx = apply(&mut b, inferred(100, "s1", t.at(1), dispatched("c1", "Bash", &json!({"command": "ls"}))));
    assert_eq!(truth(&b, 100), Phase::Working);
    assert_eq!(shown(&b, 100), Phase::Working);
    assert_eq!(open_tools(&b, 100), ["c1"]);
    assert!(publishes(&fx));
}

/// A replayed `end_turn` line on an unheard agent is grounds for idle.
#[test]
fn should_go_idle_when_a_replayed_stop_reaches_an_unheard_agent() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, Some("s1"))], t.at(0));
    let fx = apply(&mut b, inferred(100, "s1", t.at(1), stop()));
    assert_eq!(shown(&b, 100), Phase::Idle);
    assert!(publishes(&fx));
    assert!(matches!(
        agent(&b, 100).activity,
        Activity::Idle {
            after: Some(TurnOutcome::Completed),
            ..
        }
    ));
}

/// A replayed interrupt on an unheard agent: truth is `Stopped` at once, but
/// as an agent-caused, inferred escalation it waits out the dwell on screen.
#[test]
fn should_hold_grey_while_a_replayed_interrupt_serves_its_dwell() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, Some("s1"))], t.at(0));
    apply(&mut b, inferred(100, "s1", t.at(1), interrupted()));
    assert_eq!(truth(&b, 100), Phase::Stopped);
    assert_eq!(shown(&b, 100), Phase::Unknown);
    assert!(agent(&b, 100).shown.pending.is_some());
}

/// A tracked pid wins the routing, so an event from it that names a session
/// the agent does not hold is a late event, not a hint to move some other
/// agent that does hold that session.
#[test]
fn should_drop_an_event_whose_tracked_pid_disagrees_with_its_session() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1"), (200, "s2")], t.at(0));
    let fx = apply(&mut b, hook(100, "s2", t.at(1), turn_started("p1")));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(shown(&b, 100), Phase::Idle);
    assert_eq!(shown(&b, 200), Phase::Idle);
}

/// Compaction is its own axis: it starts, it ends, and a second end is a
/// no-op. It never changes the phase by itself.
#[test]
fn should_track_compaction_as_its_own_axis() {
    let t = Clock::new();
    let mut b = board_with(&[(100, "s1")], t.at(0));
    let started = EventKind::CompactionStarted {
        trigger: Trigger::Auto,
    };
    let fx = apply(&mut b, hook(100, "s1", t.at(1), started));
    assert!(matches!(
        agent(&b, 100).compaction,
        Compaction::InProgress {
            trigger: Trigger::Auto,
            ..
        }
    ));
    assert!(publishes(&fx));
    assert_eq!(shown(&b, 100), Phase::Idle);
    apply(&mut b, hook(100, "s1", t.at(2), EventKind::CompactionEnded));
    assert_eq!(agent(&b, 100).compaction, Compaction::None);
    let again = apply(&mut b, hook(100, "s1", t.at(3), EventKind::CompactionEnded));
    assert!(again.is_empty(), "{again:?}");
}
