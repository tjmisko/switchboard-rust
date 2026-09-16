//! T08 — the reducer, part II: permission gates, the join, subagents.

mod common;

use serde_json::json;

use common::*;
use switchboard::model::*;

fn ls() -> serde_json::Value {
    json!({"command": "ls", "description": "list"})
}

fn blocked_owner(g: Option<&Gate>) -> Option<Ownership> {
    match g {
        Some(Gate::Blocked { owner, .. }) => Some(*owner),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// The join, from both directions
// ---------------------------------------------------------------------------

#[test]
fn should_gate_the_running_call_when_a_prompt_arrives_after_it() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &ls())),
    );
    let fx = apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));

    assert_eq!(blocked_owner(gate_of(&b, 1, "c1")), Some(Ownership::Human));
    assert!(agent(&b, 1).prompts.unjoined.is_empty());
    assert_eq!(shown(&b, 1), Phase::Attention, "confirmed: no dwell");
    assert!(publishes(&fx));
}

#[test]
fn should_park_a_prompt_that_arrives_first_and_join_it_when_the_call_arrives() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    assert_eq!(agent(&b, 1).prompts.unjoined.len(), 1);
    assert_eq!(
        truth(&b, 1),
        Phase::Attention,
        "an unjoined human prompt already wants a person"
    );

    apply(
        &mut b,
        inferred(1, "s", t.at(3), dispatched("c1", "Bash", &ls())),
    );
    assert!(agent(&b, 1).prompts.unjoined.is_empty());
    match gate_of(&b, 1, "c1") {
        Some(Gate::Blocked { since, kind, .. }) => {
            assert_eq!(*since, t.at(2), "the gate keeps the prompt's arrival time");
            assert_eq!(*kind, GateKind::Approval);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn should_not_join_a_prompt_to_a_call_with_a_different_input() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(
            1,
            "s",
            t.at(1),
            dispatched("c1", "Bash", &json!({"command": "pwd"})),
        ),
    );
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    assert_eq!(gate_of(&b, 1, "c1"), Some(&Gate::Running));
    assert_eq!(agent(&b, 1).prompts.unjoined.len(), 1);
}

#[test]
fn should_not_join_a_prompt_to_a_call_of_a_different_tool() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Read", &ls())),
    );
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    assert_eq!(gate_of(&b, 1, "c1"), Some(&Gate::Running));
}

#[test]
fn should_join_only_one_of_two_identical_parallel_calls() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &ls())),
    );
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c2", "Bash", &ls())),
    );
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    let gated = ["c1", "c2"]
        .iter()
        .filter(|c| matches!(gate_of(&b, 1, c), Some(Gate::Blocked { .. })))
        .count();
    assert_eq!(gated, 1);
}

#[test]
fn should_publish_nothing_for_a_duplicate_parked_prompt() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), asked("Bash", &ls())));
    let fx = apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    assert!(fx.is_empty());
    assert_eq!(agent(&b, 1).prompts.unjoined.len(), 1);
}

#[test]
fn should_match_a_hashless_prompt_on_tool_name_alone() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &ls())),
    );
    let mut kind = asked("Bash", &ls());
    if let EventKind::PermissionAsked { hash, .. } = &mut kind {
        *hash = None;
    }
    apply(&mut b, hook(1, "s", t.at(2), kind));
    assert!(matches!(gate_of(&b, 1, "c1"), Some(Gate::Blocked { .. })));
}

// ---------------------------------------------------------------------------
// Lifting: liberal
// ---------------------------------------------------------------------------

#[test]
fn should_lift_attention_at_once_when_the_gated_call_settles() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &ls())),
    );
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    apply(&mut b, hook(1, "s", t.at(30), settled("c1")));
    assert_eq!(shown(&b, 1), Phase::Working);
}

#[test]
fn should_lift_the_sole_parked_prompt_when_an_unseen_call_settles() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    let fx = apply(&mut b, hook(1, "s", t.at(3), settled("c-never-dispatched")));
    assert!(agent(&b, 1).prompts.unjoined.is_empty());
    assert_eq!(shown(&b, 1), Phase::Working);
    assert!(publishes(&fx));
}

#[test]
fn should_keep_both_parked_prompts_when_an_unseen_call_settles_ambiguously() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), asked("Bash", &ls())));
    apply(
        &mut b,
        hook(1, "s", t.at(1), asked("Write", &json!({"file_path": "x"}))),
    );
    apply(&mut b, hook(1, "s", t.at(2), settled("c-unknown")));
    assert_eq!(agent(&b, 1).prompts.unjoined.len(), 2);
    assert_eq!(truth(&b, 1), Phase::Attention);
}

#[test]
fn should_lift_every_agent_prompt_when_the_turn_ends_or_a_new_one_starts() {
    let t = Clock::new();
    for closing in [stop(), turn_started("next")] {
        let mut b = board_with(&[(1, "s")], t.at(0));
        apply(&mut b, hook(1, "s", t.at(1), asked("Bash", &ls())));
        apply(&mut b, hook(1, "s", t.at(2), closing));
        assert!(agent(&b, 1).prompts.unjoined.is_empty());
        assert_ne!(truth(&b, 1), Phase::Attention);
    }
}

/// The hook and the transcript both report one prompt, and a
/// `PermissionRequest` can land between the two reports. The second report
/// is a duplicate `TurnStarted`, and a duplicate must not lift what was
/// parked after the first: T07's duplicate check runs before its clear.
#[test]
fn should_keep_a_prompt_parked_between_the_hooks_and_the_transcripts_report_of_one_prompt() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    assert_eq!(truth(&b, 1), Phase::Attention);

    let fx = apply(&mut b, inferred(1, "s", t.at(3), turn_started("p")));
    assert!(fx.is_empty(), "{fx:?}");
    assert_eq!(agent(&b, 1).prompts.unjoined.len(), 1);
    assert_eq!(truth(&b, 1), Phase::Attention);

    // A genuinely new prompt does lift it.
    apply(&mut b, hook(1, "s", t.at(4), turn_started("next")));
    assert!(agent(&b, 1).prompts.unjoined.is_empty());
}

/// Claude's `PermissionDenied` may carry no call id. A call-less answer can
/// only mean a parked prompt was dealt with: the sole parked prompt is
/// lifted; with two parked, neither is (which one?); and a gated call is
/// never released by it.
#[test]
fn should_lift_only_the_sole_parked_prompt_when_a_call_less_permission_settles() {
    let t = Clock::new();
    let answered = || EventKind::PermissionSettled {
        call: None,
        req: None,
        outcome: Decision::Allowed,
    };

    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    let fx = apply(&mut b, by_user(1, "s", t.at(3), answered()));
    assert!(publishes(&fx));
    assert!(agent(&b, 1).prompts.unjoined.is_empty());
    assert_eq!(shown(&b, 1), Phase::Working);

    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), asked("Bash", &ls())));
    apply(
        &mut b,
        hook(1, "s", t.at(1), asked("Write", &json!({"file_path": "x"}))),
    );
    let fx = apply(&mut b, by_user(1, "s", t.at(2), answered()));
    assert!(fx.is_empty(), "ambiguous: {fx:?}");
    assert_eq!(agent(&b, 1).prompts.unjoined.len(), 2);

    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &ls())),
    );
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    let fx = apply(&mut b, by_user(1, "s", t.at(3), answered()));
    assert!(fx.is_empty(), "a gated call needs its id: {fx:?}");
    assert!(matches!(gate_of(&b, 1, "c1"), Some(Gate::Blocked { .. })));
}

#[test]
fn should_release_the_gate_when_the_prompt_is_allowed_and_drop_the_call_when_denied() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &ls())),
    );
    apply(
        &mut b,
        hook(
            1,
            "s",
            t.at(1),
            dispatched("c2", "Bash", &json!({"command": "rm"})),
        ),
    );
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &ls())));
    apply(
        &mut b,
        hook(1, "s", t.at(2), asked("Bash", &json!({"command": "rm"}))),
    );

    apply(
        &mut b,
        by_user(
            1,
            "s",
            t.at(3),
            EventKind::PermissionSettled {
                call: Some("c1".into()),
                req: None,
                outcome: Decision::Allowed,
            },
        ),
    );
    assert_eq!(gate_of(&b, 1, "c1"), Some(&Gate::Running));
    assert_eq!(truth(&b, 1), Phase::Attention, "c2 still waits");

    apply(
        &mut b,
        by_user(
            1,
            "s",
            t.at(4),
            EventKind::PermissionSettled {
                call: Some("c2".into()),
                req: None,
                outcome: Decision::Denied { by: Denier::User },
            },
        ),
    );
    assert_eq!(open_tools(&b, 1), ["c1"]);
    assert_eq!(shown(&b, 1), Phase::Working);
}

// ---------------------------------------------------------------------------
// Question tools, unowned gates
// ---------------------------------------------------------------------------

#[test]
fn should_create_a_blocked_call_when_a_question_tool_is_its_own_dispatch() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), question("toolu_ask", "AskUserQuestion")),
    );
    match gate_of(&b, 1, "toolu_ask") {
        Some(Gate::Blocked { kind, owner, .. }) => {
            assert_eq!(*kind, GateKind::Question);
            assert_eq!(*owner, Ownership::Human);
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(shown(&b, 1), Phase::Attention);
}

#[test]
fn should_gate_an_already_dispatched_call_when_its_id_arrives_with_the_ask() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        inferred(
            1,
            "s",
            t.at(1),
            dispatched("toolu_ask", "AskUserQuestion", &json!({})),
        ),
    );
    let fx = apply(
        &mut b,
        hook(1, "s", t.at(2), question("toolu_ask", "AskUserQuestion")),
    );
    assert!(publishes(&fx));
    assert_eq!(open_tools(&b, 1), ["toolu_ask"]);
    let again = apply(
        &mut b,
        hook(1, "s", t.at(3), question("toolu_ask", "AskUserQuestion")),
    );
    assert!(again.is_empty(), "already blocked");
}

/// Claude announces one question twice: `PreToolUse` with the call id, then
/// `PermissionRequest` with the same input and no id. The second is the
/// same prompt said again, not a second question, so answering the call
/// must leave nothing parked behind it.
#[test]
fn should_treat_a_call_less_ask_for_an_already_gated_call_as_the_same_prompt() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    let input = json!({"questions": [{"question": "which?"}]});
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    let mut first = question("toolu_ask", "AskUserQuestion");
    if let EventKind::PermissionAsked { hash, .. } = &mut first {
        *hash = Some(common::hash(&input));
    }
    apply(&mut b, hook(1, "s", t.at(2), first));

    let mut second = asked("AskUserQuestion", &input);
    if let EventKind::PermissionAsked { kind, .. } = &mut second {
        *kind = GateKind::Question;
    }
    let fx = apply(&mut b, hook(1, "s", t.at(3), second));
    assert!(fx.is_empty(), "the same question, announced twice");
    assert!(agent(&b, 1).prompts.unjoined.is_empty());
    assert_eq!(truth(&b, 1), Phase::Attention);

    // The operator answers: PostToolUse carries the id from PreToolUse.
    apply(&mut b, hook(1, "s", t.at(90), settled("toolu_ask")));
    assert_eq!(shown(&b, 1), Phase::Working, "nothing parked keeps it red");
}

/// Two genuinely different questions with identical inputs, each announced
/// with its id, are two gates: answering one leaves the other open.
#[test]
fn should_keep_the_second_of_two_identical_questions_open_when_the_first_is_answered() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    let input = json!({"questions": [{"question": "which?"}]});
    let ask_with = |call: &str| {
        let mut k = question(call, "AskUserQuestion");
        if let EventKind::PermissionAsked { hash, .. } = &mut k {
            *hash = Some(common::hash(&input));
        }
        k
    };
    apply(&mut b, hook(1, "s", t.at(1), ask_with("q1")));
    apply(
        &mut b,
        hook(1, "s", t.at(1), asked("AskUserQuestion", &input)),
    );
    apply(&mut b, hook(1, "s", t.at(2), ask_with("q2")));
    apply(
        &mut b,
        hook(1, "s", t.at(2), asked("AskUserQuestion", &input)),
    );
    assert_eq!(open_tools(&b, 1), ["q1", "q2"]);
    assert!(agent(&b, 1).prompts.unjoined.is_empty());

    apply(&mut b, hook(1, "s", t.at(3), settled("q1")));
    assert_eq!(truth(&b, 1), Phase::Attention, "q2 still waits");
    apply(&mut b, hook(1, "s", t.at(4), settled("q2")));
    assert_eq!(shown(&b, 1), Phase::Working);
}

#[test]
fn should_render_an_unowned_gate_grey_never_red_even_mid_turn() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    apply(
        &mut b,
        hook(1, "s", t.at(2), asked_unowned("call-1", "exec_command")),
    );
    assert_eq!(truth(&b, 1), Phase::Unknown);
    assert_eq!(shown(&b, 1).lamp(), Lamp::Grey);
}

// ---------------------------------------------------------------------------
// Subagents
// ---------------------------------------------------------------------------

fn start_sub(b: &mut Board, t: &Clock, at: u64, sub: &str) -> Vec<Effect> {
    apply(
        b,
        hook(
            1,
            "s",
            t.at(at),
            EventKind::SubagentStarted {
                subagent: sub.into(),
                kind: SubagentKind::Task,
            },
        ),
    )
}

fn end_sub(b: &mut Board, t: &Clock, at: u64, sub: &str) -> Vec<Effect> {
    apply(
        b,
        hook(
            1,
            "s",
            t.at(at),
            EventKind::SubagentEnded {
                subagent: sub.into(),
                outcome: SubagentOutcome::Completed,
            },
        ),
    )
}

#[test]
fn should_delegate_while_an_event_created_subagent_runs_and_idle_after_it_ends() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    start_sub(&mut b, &t, 1, "a1");
    assert_eq!(truth(&b, 1), Phase::Delegating);
    assert!(start_sub(&mut b, &t, 2, "a1").is_empty(), "duplicate start");
    end_sub(&mut b, &t, 3, "a1");
    assert_eq!(truth(&b, 1), Phase::Idle);
    assert!(end_sub(&mut b, &t, 4, "a1").is_empty(), "duplicate end");
}

#[test]
fn should_raise_attention_from_a_subagent_gate_and_keep_it_through_the_agents_stop() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    start_sub(&mut b, &t, 1, "a1");
    apply(
        &mut b,
        from_subagent(
            1,
            "s",
            "a1",
            t.at(2),
            question("toolu_q", "AskUserQuestion"),
        ),
    );
    assert_eq!(shown(&b, 1), Phase::Attention);

    // The agent's own Stop must never clear a subagent's gate.
    apply(&mut b, hook(1, "s", t.at(3), stop()));
    assert_eq!(truth(&b, 1), Phase::Attention);

    // The subagent moving again lifts it.
    apply(
        &mut b,
        from_subagent(1, "s", "a1", t.at(4), settled("toolu_q")),
    );
    assert_eq!(shown(&b, 1), Phase::Delegating);
}

#[test]
fn should_park_a_prompt_from_an_unknown_subagent_and_lift_it_when_that_subagent_ends() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        from_subagent(1, "s", "ghost", t.at(1), asked("Bash", &ls())),
    );
    assert_eq!(truth(&b, 1), Phase::Attention);
    apply(&mut b, hook(1, "s", t.at(2), stop()));
    assert_eq!(
        truth(&b, 1),
        Phase::Attention,
        "not the agent's prompt to clear"
    );
    end_sub(&mut b, &t, 3, "ghost");
    assert_eq!(truth(&b, 1), Phase::Idle);
}

/// A subagent's turn is not the agent's turn. 47 of 56 captured
/// `StopFailure` payloads were a child's; the parent was still working.
#[test]
fn should_keep_the_agent_working_and_mark_the_row_failed_when_a_subagents_turn_fails() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    start_sub(&mut b, &t, 2, "a1");
    assert_eq!(truth(&b, 1), Phase::Working);

    let failed = turn_ended(
        Some("p"),
        TurnOutcome::Failed {
            class: ErrorClass::RateLimit,
        },
    );
    let fx = apply(&mut b, from_subagent(1, "s", "a1", t.at(3), failed));
    assert!(publishes(&fx), "a failed child is a fact worth showing");
    assert_eq!(truth(&b, 1), Phase::Working);
    assert_eq!(shown(&b, 1), Phase::Working);
    assert!(matches!(agent(&b, 1).activity, Activity::Turn { .. }));
    assert_eq!(
        agent(&b, 1).subagents.rows[&SubagentId::from("a1")].status,
        SubagentStatus::Failed
    );

    // The agent's own Stop still ends the agent's turn as completed.
    apply(&mut b, hook(1, "s", t.at(4), stop()));
    assert_eq!(
        truth(&b, 1),
        Phase::Idle,
        "a failed child is not delegation"
    );
}

/// A subagent's answered prompt lifts its gate exactly as its tool settling
/// does: the child is moving again.
#[test]
fn should_lift_a_subagents_gate_when_its_permission_is_settled() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    start_sub(&mut b, &t, 1, "a1");
    apply(
        &mut b,
        from_subagent(1, "s", "a1", t.at(2), asked("Bash", &ls())),
    );
    assert_eq!(truth(&b, 1), Phase::Attention);
    let settled = EventKind::PermissionSettled {
        call: None,
        req: None,
        outcome: Decision::Allowed,
    };
    let fx = apply(&mut b, from_subagent(1, "s", "a1", t.at(3), settled));
    assert!(publishes(&fx));
    assert_eq!(shown(&b, 1), Phase::Delegating);
}

/// Escape inside a child's tool is reported to the child. The agent learns
/// about its own interruption from its own signals.
#[test]
fn should_not_end_the_agents_turn_when_a_subagents_tool_is_interrupted() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    start_sub(&mut b, &t, 2, "a1");
    let interrupted = EventKind::ToolSettled {
        call: "c-child".into(),
        outcome: ToolOutcome::Failed { interrupt: true },
    };
    apply(&mut b, from_subagent(1, "s", "a1", t.at(3), interrupted));
    assert_eq!(truth(&b, 1), Phase::Working);
}

/// What a child says about its own session, compaction or cwd is dropped:
/// none of it describes the agent.
#[test]
fn should_ignore_a_subagents_session_compaction_and_cwd_events() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    start_sub(&mut b, &t, 1, "a1");
    for kind in [
        EventKind::CompactionStarted {
            trigger: Trigger::Auto,
        },
        EventKind::CwdChanged {
            to: "/elsewhere".into(),
        },
        turn_started("child-prompt"),
        stop(),
    ] {
        let fx = apply(&mut b, from_subagent(1, "s", "a1", t.at(2), kind));
        assert!(fx.is_empty(), "no effects");
    }
    assert_eq!(agent(&b, 1).compaction, Compaction::None);
    assert!(!agent(&b, 1).cwd.ends_with("elsewhere"));
    assert_eq!(truth(&b, 1), Phase::Delegating);
}

/// A prompt parked before its subagent had a row moves onto that row when
/// the subagent dispatches the matching call. Asking again on a row that is
/// already gated changes nothing.
#[test]
fn should_move_a_parked_prompt_onto_the_row_when_the_subagent_dispatches_the_call() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        from_subagent(1, "s", "a1", t.at(1), asked("Bash", &ls())),
    );
    assert_eq!(agent(&b, 1).prompts.unjoined.len(), 1);
    start_sub(&mut b, &t, 2, "a1");
    let fx = apply(
        &mut b,
        from_subagent(1, "s", "a1", t.at(3), dispatched("c1", "Bash", &ls())),
    );
    assert!(publishes(&fx));
    assert!(agent(&b, 1).prompts.unjoined.is_empty());
    let row = &agent(&b, 1).subagents.rows[&SubagentId::from("a1")];
    assert!(matches!(row.gate, Some(Gate::Blocked { since, .. }) if since == t.at(1)));
    assert_eq!(truth(&b, 1), Phase::Attention);

    let again = apply(
        &mut b,
        from_subagent(1, "s", "a1", t.at(4), asked("Bash", &ls())),
    );
    assert!(again.is_empty(), "already gated");

    // An unrelated dispatch from that child, with nothing parked, is silent.
    let quiet = apply(
        &mut b,
        from_subagent(1, "s", "a1", t.at(5), dispatched("c2", "Read", &ls())),
    );
    assert!(quiet.is_empty());
}

#[test]
fn should_not_let_events_add_or_remove_rows_when_the_panel_owns_them() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    {
        let a = b.agents.get_mut(&key(1)).unwrap();
        a.subagents.source = SubagentSource::Panel;
    }
    assert!(start_sub(&mut b, &t, 1, "a1").is_empty());
    assert!(agent(&b, 1).subagents.rows.is_empty());
}
