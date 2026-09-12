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

// Delete once the tests are written.
#![allow(unused)]

mod common;

use std::path::PathBuf;

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
    todo!("write this test")
}

/// `process: None` routes by session. With two agents, only the one owning
/// that session moves.
#[test]
fn should_route_by_session_when_the_event_carries_no_process() {
    todo!("write this test")
}

/// A pid the board doesn't track falls back to the session too.
#[test]
fn should_route_by_session_when_the_process_is_not_tracked() {
    todo!("write this test")
}

/// An agent discovered without a session adopts the first event's session and
/// asks for its transcript to be watched. Check the exact `WatchTranscript`
/// effect, cwd included.
#[test]
fn should_adopt_the_session_and_watch_its_transcript_when_the_agent_had_none() {
    todo!("write this test")
}

/// After `SessionStarted{Clear}` moves the agent to a new session, an event
/// still carrying the old session is a no-op: no effects, no change.
#[test]
fn should_ignore_a_late_event_for_the_previous_session_after_clear() {
    todo!("write this test")
}

/// A rotation mid-turn wipes the old conversation. The agent is idle with no
/// outcome, the title updates, the new transcript is watched, and the chip
/// shows idle.
#[test]
fn should_reset_the_conversation_and_watch_the_new_transcript_when_the_session_rotates() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// Discovery and the first event
// ---------------------------------------------------------------------------

/// Discovered and never heard from: truth and shown are both `Unknown`, which
/// paints grey.
#[test]
fn should_show_unknown_for_a_discovered_agent_until_it_is_heard_from() {
    todo!("write this test")
}

/// `SessionStarted{Startup}` on an unobserved agent shows `Idle`, publishes,
/// and records exactly one history entry.
#[test]
fn should_go_idle_on_session_start() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// Turns
// ---------------------------------------------------------------------------

/// A turn starting shows `Working` immediately. The history entry is exact:
/// from `Idle`, to `Working`, `held` equal to the time since the last change.
#[test]
fn should_show_working_at_once_and_record_it_when_a_turn_starts() {
    todo!("write this test")
}

/// The hook and the transcript both report the same prompt. The second report
/// produces no effects at all.
#[test]
fn should_publish_nothing_when_the_same_turn_starts_twice() {
    todo!("write this test")
}

/// A different prompt key mid-turn keeps the open calls and takes the new key.
#[test]
fn should_keep_open_calls_when_a_new_prompt_starts_mid_turn() {
    todo!("write this test")
}

/// `TurnEnded` abandons every open call, goes idle, and shows it immediately
/// (de-escalation never waits).
#[test]
fn should_go_idle_and_abandon_open_calls_when_the_turn_ends() {
    todo!("write this test")
}

/// A `TurnEnded` whose key names a different turn than the open one is a
/// no-op.
#[test]
fn should_ignore_a_turn_end_that_names_a_different_turn() {
    todo!("write this test")
}

/// Escape: `Working` → `Stopped` is an escalation, and it shows immediately
/// because the user caused it.
#[test]
fn should_show_stopped_at_once_when_the_user_interrupts() {
    todo!("write this test")
}

/// The next prompt after an interrupt clears `Stopped`.
#[test]
fn should_clear_stopped_when_the_next_turn_starts() {
    todo!("write this test")
}

/// An agent-caused, *inferred* turn failure: truth is `Blocked`, the screen
/// still shows `Working`, and an escalation is pending.
#[test]
fn should_hold_the_screen_while_an_inferred_turn_failure_serves_its_dwell() {
    todo!("write this test")
}

/// The same failure, *confirmed*, shows `Blocked` immediately.
#[test]
fn should_show_blocked_at_once_when_the_turn_failure_is_confirmed() {
    todo!("write this test")
}

/// A dispatch while idle starts a turn with no key.
#[test]
fn should_start_a_turn_when_a_tool_is_dispatched_while_idle() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// Tools
// ---------------------------------------------------------------------------

/// The same call dispatched twice (hook, then transcript) produces no effects.
#[test]
fn should_publish_nothing_when_the_same_call_is_dispatched_twice() {
    todo!("write this test")
}

/// Two calls open, one settles: the other stays, and the agent is still
/// working.
#[test]
fn should_remove_only_the_settled_call_and_keep_working() {
    todo!("write this test")
}

/// A settle for a call never dispatched (with no prompts parked) publishes
/// nothing.
#[test]
fn should_publish_nothing_when_an_unknown_call_settles() {
    todo!("write this test")
}

/// `ToolSettled{Failed{interrupt: true}}` from the user shows `Stopped`.
#[test]
fn should_end_the_turn_as_interrupted_when_a_tool_fails_by_interrupt() {
    todo!("write this test")
}

/// A plain failure removes the call but keeps the turn.
#[test]
fn should_keep_the_turn_when_a_tool_merely_fails() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// Death, cwd, bookkeeping
// ---------------------------------------------------------------------------

/// `Died` removes only that agent, emits `Forget` and `AgentGone`, and
/// publishes. A second `Died` for the same process is a no-op.
#[test]
fn should_remove_the_agent_forget_it_and_record_it_when_it_dies() {
    todo!("write this test")
}

/// `CwdChanged` updates `cwd` and `workspace`. The same path again is a no-op.
#[test]
fn should_follow_cwd_changes_and_rename_the_workspace() {
    todo!("write this test")
}

/// `SessionEnded` is not liveness. The agent stays and nothing is published.
#[test]
fn should_publish_nothing_for_session_end_because_it_is_not_liveness() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// yours: at least four more
// ---------------------------------------------------------------------------
