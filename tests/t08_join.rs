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
