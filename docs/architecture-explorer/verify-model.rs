//! Generates model-parity.json by executing the actual golden library.
//!
//! Run this as a binary in a temporary Cargo project with dependencies:
//! switchboard = { package = "switchboard-rust", path = "<golden-root>" }
//! serde_json = "1"
//!
//! The probe deliberately constructs model facts directly: it checks the
//! explorer's phase fold and display dwell, not adapters, routing, or event
//! transition correctness. Real hooks are not emitted and no I/O driver runs.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, UNIX_EPOCH};

use serde_json::{Value, json};
use switchboard::model::*;
use switchboard::reduce;

fn agent_with_facts(
    now: Instant,
    activity: &str,
    gate: &str,
    outcome: &str,
    children: bool,
    waits: bool,
) -> Agent {
    let mut agent = reduce::discovered(
        ProcessRow {
            key: ProcKey {
                pid: 42,
                start: 100,
            },
            provider: Provider::Claude,
            cwd: PathBuf::from("/example/project"),
            session: Some(SessionId::from("example-session")),
            tty: None,
            cpu_ticks: 0,
        },
        now,
    );

    let after = match outcome {
        "completed" => TurnOutcome::Completed,
        "interrupted" => TurnOutcome::Interrupted { killed: None },
        "failed" => TurnOutcome::Failed {
            class: ErrorClass::ServerError,
        },
        other => panic!("unknown outcome: {other}"),
    };
    agent.activity = match activity {
        "turn" => Activity::Turn {
            key: None,
            started: now,
            tools: BTreeMap::new(),
            last_event: now,
        },
        "idle" => Activity::Idle {
            since: now,
            after: Some(after),
        },
        "unobserved" => Activity::Unobserved { since: now },
        other => panic!("unknown activity: {other}"),
    };

    // Unjoined prompts are an independent axis. Putting a gate in the turn's
    // tool map would make it impossible to check idle/unobserved + gate.
    let owner = match gate {
        "none" => None,
        "human" => Some(Ownership::Human),
        "unknown" => Some(Ownership::Unknown),
        "machine" => Some(Ownership::Machine),
        other => panic!("unknown gate owner: {other}"),
    };
    if let Some(owner) = owner {
        agent.prompts.unjoined.push(Unjoined {
            actor: Actor::Agent,
            tool: ToolName::from("Bash"),
            hash: None,
            since: now,
            owner,
            kind: GateKind::Approval,
        });
    }

    if children {
        agent.subagents.rows.insert(
            SubagentId::from("child-1"),
            Subagent {
                kind: SubagentKind::Task,
                status: SubagentStatus::Running,
                parent: None,
                started: UNIX_EPOCH,
                label: "example child".into(),
                tokens: 0,
                gate: None,
                last_seen: now,
            },
        );
    }
    if waits {
        agent.waits.insert(
            WaitId("wait-1".into()),
            Wait {
                kind: WaitKind::BackgroundShell,
                armed: now,
                expected_by: None,
                expires: now + Duration::from_secs(1800),
            },
        );
    }
    agent
}

fn phase_cases(now: Instant) -> Vec<Value> {
    let mut cases = Vec::new();
    for activity in ["turn", "idle", "unobserved"] {
        for gate in ["none", "human", "unknown", "machine"] {
            for outcome in ["completed", "interrupted", "failed"] {
                for limit in [false, true] {
                    for children in [false, true] {
                        for waits in [false, true] {
                            let agent =
                                agent_with_facts(now, activity, gate, outcome, children, waits);
                            let limits = limit.then_some(Limits {
                                status: LimitStatus::Rejected,
                                resets_at: None,
                                window: None,
                                used_percent: None,
                            });
                            let expected = agent.phase(limits.as_ref());
                            cases.push(json!({
                                "input": {
                                    "activity": activity,
                                    "gate": gate,
                                    "outcome": outcome,
                                    "limit": limit,
                                    "children": children,
                                    "waits": waits,
                                },
                                "expected": expected.as_str(),
                            }));
                        }
                    }
                }
            }
        }
    }
    assert_eq!(cases.len(), 288);
    cases
}

fn shown_cases(now: Instant) -> Vec<Value> {
    let mut cases = Vec::new();
    for truth in Phase::ALL {
        for (cause_name, cause) in [("agent", Cause::Agent), ("user", Cause::User)] {
            for (confidence_name, confidence) in [
                ("confirmed", Confidence::Confirmed),
                ("inferred", Confidence::Inferred),
                ("snapshot", Confidence::Snapshot),
            ] {
                for elapsed in [0.0_f64, 0.5, 5.5, 6.0, 9.5, 10.0, 12.0] {
                    let mut shown = Shown::new(Phase::Working, now);
                    shown.settle(truth, cause, confidence, now);
                    shown.settle(
                        truth,
                        cause,
                        confidence,
                        now + Duration::from_secs_f64(elapsed),
                    );
                    cases.push(json!({
                        "truth": truth.as_str(),
                        "cause": cause_name,
                        "confidence": confidence_name,
                        "elapsed": elapsed,
                        "expected": shown.phase.as_str(),
                        "pending": shown.pending.map(|value| value.to.as_str()),
                    }));
                }
            }
        }
    }
    assert_eq!(cases.len(), 336);
    cases
}

fn main() {
    let now = Instant::now();
    let output = json!({
        "phaseCases": phase_cases(now),
        "shownCases": shown_cases(now),
    });
    println!("{}", serde_json::to_string_pretty(&output).unwrap());
}
