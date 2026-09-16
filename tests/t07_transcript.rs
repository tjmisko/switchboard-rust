//! T07 — the transcript adapter.

mod common;

use std::time::Instant;

use serde_json::{Value, json};

use common::{fixture, key};
use switchboard::ingest::Input;
use switchboard::ingest::claude::{LineCtx, line};
use switchboard::ingest::hash::input_hash;
use switchboard::model::*;
use switchboard::reduce::default_account;

fn ctx(confidence: Confidence) -> LineCtx {
    LineCtx {
        process: key(6002),
        session: SessionId::from("watched-session"),
        account: default_account(),
        now: Instant::now(),
        confidence,
    }
}

fn lines() -> Vec<Value> {
    std::fs::read_to_string(fixture("claude/transcript/session.jsonl"))
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

/// Every input the fixture produces, in order.
fn all_inputs(confidence: Confidence) -> Vec<Input> {
    let c = ctx(confidence);
    lines().iter().flat_map(|v| line(v, &c)).collect()
}

fn kinds(inputs: &[Input]) -> Vec<String> {
    inputs
        .iter()
        .map(|i| match i {
            Input::Event(e) => match &e.kind {
                EventKind::TurnStarted { origin, .. } => format!("TurnStarted({origin:?})"),
                EventKind::TurnEnded { outcome, .. } => format!("TurnEnded({outcome:?})"),
                EventKind::ToolDispatched { call, tool, .. } => {
                    format!("ToolDispatched({call},{tool})")
                }
                EventKind::ToolSettled { call, outcome } => {
                    format!("ToolSettled({call},{outcome:?})")
                }
                other => format!("{other:?}"),
            },
            Input::Snapshot(s) => match &s.kind {
                SnapshotKind::Limits { value, .. } => format!("Limits({:?})", value.status),
                other => format!("{other:?}"),
            },
        })
        .collect()
}

#[test]
fn should_turn_the_fixture_into_exactly_this_sequence() {
    let got = kinds(&all_inputs(Confidence::Confirmed));
    let expected = [
        "TurnStarted(Human)",
        "ToolDispatched(toolu_01CT,Bash)",
        "ToolSettled(toolu_01CT,Ok { ms: None })",
        "TurnEnded(Completed)",
        "TurnStarted(Human)",
        "ToolDispatched(toolu_02RD,Read)",
        "ToolDispatched(toolu_02GR,Grep)",
        "TurnEnded(Interrupted { killed: Some(MessageId(\"msg_0003\")) })",
        "TurnStarted(Human)",
        "ToolDispatched(toolu_03RM,Bash)",
        "ToolSettled(toolu_03RM,Denied { by: User })",
        "TurnEnded(Failed { class: RateLimit })",
        "Limits(Rejected)",
        "TurnStarted(TaskNotification)",
    ];
    assert_eq!(got, expected);
}

#[test]
fn should_stamp_every_event_with_the_watchers_process_session_and_confidence() {
    for input in all_inputs(Confidence::Inferred) {
        if let Input::Event(e) = input {
            assert_eq!(e.process, Some(key(6002)));
            assert_eq!(e.session.as_str(), "watched-session");
            assert_eq!(e.confidence, Confidence::Inferred);
            assert_eq!(e.actor, Actor::Agent);
            assert!(e.at.claimed.is_some(), "timestamp parsed");
        }
    }
}

#[test]
fn should_attribute_interrupts_denials_by_the_user_and_typed_prompts_to_the_user() {
    let inputs = all_inputs(Confidence::Confirmed);
    let user_caused: Vec<String> = inputs
        .iter()
        .filter_map(|i| match i {
            Input::Event(e) if e.cause == Cause::User => Some(e),
            _ => None,
        })
        .map(|e| format!("{:?}", std::mem::discriminant(&e.kind)))
        .collect();
    assert_eq!(
        user_caused.len(),
        5,
        "3 prompts, 1 interrupt, 1 user denial"
    );
}

#[test]
fn should_hash_a_tool_use_input_the_same_way_the_forwarder_would() {
    let inputs = all_inputs(Confidence::Confirmed);
    let hash = inputs.iter().find_map(|i| match i {
        Input::Event(Event {
            kind: EventKind::ToolDispatched { call, hash, .. },
            ..
        }) if call.as_str() == "toolu_03RM" => Some(*hash),
        _ => None,
    });
    assert_eq!(hash, Some(input_hash(&json!({"command": "rm -rf build/"}))));
}

#[test]
fn should_key_the_prompt_turn_from_prompt_id() {
    let inputs = all_inputs(Confidence::Confirmed);
    let first = inputs.iter().find_map(|i| match i {
        Input::Event(Event {
            kind: EventKind::TurnStarted { turn, .. },
            ..
        }) => turn.clone(),
        _ => None,
    });
    assert_eq!(first, Some(TurnKey::Prompt(PromptId::from("p-0001"))));
}

/// The interrupt line carries the `promptId` of the turn it cut short, and
/// the adapter keys the `TurnEnded` on it exactly as it keys a prompt. An
/// interrupt with no `promptId` ends whatever turn is open.
#[test]
fn should_key_the_interrupt_from_the_prompt_id_when_the_line_carries_one() {
    let inputs = all_inputs(Confidence::Confirmed);
    let interrupt = inputs.iter().find_map(|i| match i {
        Input::Event(Event {
            kind:
                EventKind::TurnEnded {
                    turn,
                    outcome: TurnOutcome::Interrupted { .. },
                },
            ..
        }) => Some(turn.clone()),
        _ => None,
    });
    assert_eq!(
        interrupt,
        Some(Some(TurnKey::Prompt(PromptId::from("p-0002"))))
    );

    let keyless = json!({
        "type": "user",
        "interruptedMessageId": "msg_9",
        "message": {"content": [{"type": "text", "text": "[Request interrupted by user]"}]}
    });
    let out = line(&keyless, &ctx(Confidence::Confirmed));
    let [Input::Event(e)] = out.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(e.cause, Cause::User);
    assert!(matches!(
        &e.kind,
        EventKind::TurnEnded {
            turn: None,
            outcome: TurnOutcome::Interrupted {
                killed: Some(killed)
            }
        } if killed.as_str() == "msg_9"
    ));
}

/// A prompt line with no `origin` is a human prompt. Most typed prompts in
/// real transcripts carry no `origin` at all, and every machine-started one
/// names its `origin.kind`, so absence is evidence of a person here, unlike
/// the hook side where an absent `source` means nothing.
#[test]
fn should_read_an_absent_origin_as_human_when_the_line_is_a_plain_prompt() {
    let v = json!({
        "type": "user",
        "promptId": "p-old",
        "message": {"role": "user", "content": "a typed line with no origin"}
    });
    let out = line(&v, &ctx(Confidence::Confirmed));
    let [Input::Event(e)] = out.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(e.cause, Cause::User);
    let EventKind::TurnStarted { turn, origin } = &e.kind else {
        panic!("{:?}", e.kind)
    };
    assert_eq!(*origin, Origin::Human);
    assert_eq!(*turn, Some(TurnKey::Prompt(PromptId::from("p-old"))));
}

#[test]
fn should_classify_a_denial_by_rule_as_agent_caused() {
    let v = json!({
        "type": "user",
        "toolDenialKind": "permission-rule",
        "message": {"content": [{"type": "tool_result", "tool_use_id": "t1", "is_error": true}]}
    });
    let out = line(&v, &ctx(Confidence::Confirmed));
    let [Input::Event(e)] = out.as_slice() else {
        panic!("{out:?}")
    };
    assert_eq!(e.cause, Cause::Agent);
    assert!(matches!(
        e.kind,
        EventKind::ToolSettled {
            outcome: ToolOutcome::Denied {
                by: Denier::PermissionRule
            },
            ..
        }
    ));
}

#[test]
fn should_treat_an_unknown_denial_kind_as_a_policy_denial_not_a_user_one() {
    let v = json!({
        "type": "user",
        "toolDenialKind": "some-new-kind",
        "message": {"content": [{"type": "tool_result", "tool_use_id": "t1"}]}
    });
    let out = line(&v, &ctx(Confidence::Confirmed));
    assert!(matches!(
        &out[..],
        [Input::Event(Event {
            cause: Cause::Agent,
            ..
        })]
    ));
}

#[test]
fn should_settle_a_failed_tool_result_as_failed() {
    let v = json!({
        "type": "user",
        "message": {"content": [
            {"type": "text", "text": "ignored"},
            {"type": "tool_result", "tool_use_id": "t1", "is_error": true},
            {"type": "tool_result", "tool_use_id": "t2"},
            {"type": "tool_result"}
        ]}
    });
    let got = kinds(&line(&v, &ctx(Confidence::Confirmed)));
    assert_eq!(
        got,
        [
            "ToolSettled(t1,Failed { interrupt: false })",
            "ToolSettled(t2,Ok { ms: None })"
        ]
    );
}

#[test]
fn should_produce_nothing_for_malformed_or_irrelevant_lines() {
    let c = ctx(Confidence::Confirmed);
    for v in [
        json!(null),
        json!([]),
        json!({"type": "assistant"}),
        json!({"type": "assistant", "message": {"content": "not blocks"}}),
        json!({"type": "user", "message": {"content": 42}}),
        json!({"type": "system", "subtype": "turn_duration"}),
        json!({"type": "user", "isMeta": true, "message": {"content": "caveat"}}),
        json!({"type": "assistant", "message": {"content": [{"type": "tool_use", "name": "Bash"}]}}),
    ] {
        assert!(line(&v, &c).is_empty(), "{v}");
    }
}
