//! T09 — snapshots, time, and the view.
//!
//! Two groups, Discovery and The view, exercise provided code and pass
//! before T09 starts. They are regression checks on `discover`,
//! `discovered` and `view.rs`, not on anything you write here.

mod common;

use std::time::{Duration, SystemTime};

use serde_json::json;

use common::*;
use switchboard::model::*;
use switchboard::reduce::{self, SUBAGENT_STALE, TURN_STALE, WAIT_EXPIRY, next_deadline, tick};
use switchboard::view::{View, describe, reason};

fn snapshot(taken: std::time::Instant, kind: SnapshotKind) -> Snapshot {
    Snapshot { taken, kind }
}

fn sub_row(id: &str, status: SubagentStatus) -> SubagentRow {
    SubagentRow {
        subagent: id.into(),
        kind: SubagentKind::Task,
        status,
        started: SystemTime::UNIX_EPOCH,
        label: id.into(),
        tokens: 10,
    }
}

// ---------------------------------------------------------------------------
// Discovery (provided code: `discover` and `discovered` ship with the
// scaffold, so this group already passes)
// ---------------------------------------------------------------------------

#[test]
fn should_add_each_new_process_once_and_watch_those_with_a_session() {
    let t = Clock::new();
    let mut b = Board::default();
    let fx = reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(0),
            SnapshotKind::Processes(vec![row(1, Some("s1")), row(2, None)]),
        ),
    );
    assert_eq!(b.agents.len(), 2);
    let watches: Vec<u32> = fx
        .iter()
        .filter_map(|e| match e {
            Effect::WatchTranscript { process, .. } => Some(process.pid),
            _ => None,
        })
        .collect();
    assert_eq!(watches, [1]);
    assert_eq!(history(&fx).len(), 2);
    assert!(publishes(&fx));

    let again = reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(1),
            SnapshotKind::Processes(vec![row(1, Some("s1")), row(2, None)]),
        ),
    );
    assert!(again.is_empty(), "{again:?}");
}

#[test]
fn should_adopt_a_session_a_later_sweep_discovers() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(2, None)], t.at(0));
    let fx = reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(1),
            SnapshotKind::Processes(vec![row(2, Some("found"))]),
        ),
    );
    assert_eq!(agent(&b, 2).session.as_deref(), Some("found"));
    assert!(
        fx.iter()
            .any(|e| matches!(e, Effect::WatchTranscript { .. }))
    );
}

/// The session file's `name` is the name the operator recognises, and no
/// captured hook carries one. A sweep sets it, and a rename lands on the
/// next sweep.
#[test]
fn should_name_an_agent_from_the_sweep_and_follow_a_rename() {
    let t = Clock::new();
    let mut b = Board::default();
    let named = |name: &str| {
        let mut r = row(1, Some("s"));
        r.name = Some(name.into());
        r
    };
    reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(0),
            SnapshotKind::Processes(vec![named("switchboard-rust-7f")]),
        ),
    );
    assert_eq!(agent(&b, 1).title.as_deref(), Some("switchboard-rust-7f"));

    let again = reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(1),
            SnapshotKind::Processes(vec![named("switchboard-rust-7f")]),
        ),
    );
    assert!(again.is_empty(), "the same name again changes nothing");

    let renamed = reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(2),
            SnapshotKind::Processes(vec![named("capture-fixes")]),
        ),
    );
    assert!(publishes(&renamed));
    assert_eq!(agent(&b, 1).title.as_deref(), Some("capture-fixes"));

    // A file that stops naming the session does not unname the agent.
    reduce::apply_snapshot(
        &mut b,
        snapshot(t.at(3), SnapshotKind::Processes(vec![row(1, Some("s"))])),
    );
    assert_eq!(agent(&b, 1).title.as_deref(), Some("capture-fixes"));
}

#[test]
fn should_not_remove_an_agent_that_is_missing_from_a_sweep() {
    // Only `Died` ends an agent.
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    reduce::apply_snapshot(&mut b, snapshot(t.at(1), SnapshotKind::Processes(vec![])));
    assert!(b.agents.contains_key(&key(1)));
}

// ---------------------------------------------------------------------------
// Limits
// ---------------------------------------------------------------------------

#[test]
fn should_hold_blocked_for_one_snapshot_dwell_when_the_account_is_rejected() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s1"), (2, "s2")], t.at(0));
    reduce::apply_snapshot(&mut b, limits_at(LimitStatus::Rejected, t.at(1)));
    for pid in [1, 2] {
        assert_eq!(truth(&b, pid), Phase::Blocked, "account-wide");
        assert_eq!(shown(&b, pid), Phase::Idle, "snapshot evidence waits");
    }
    tick(&mut b, t.at(1) + Shown::SNAPSHOT_DWELL);
    for pid in [1, 2] {
        assert_eq!(shown(&b, pid), Phase::Blocked);
    }
}

#[test]
fn should_drop_a_limits_snapshot_older_than_the_one_held() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    reduce::apply_snapshot(&mut b, limits_at(LimitStatus::Allowed, t.at(5)));
    let fx = reduce::apply_snapshot(&mut b, limits_at(LimitStatus::Rejected, t.at(4)));
    assert!(fx.is_empty());
    assert_eq!(truth(&b, 1), Phase::Idle);
}

#[test]
fn should_publish_nothing_when_limits_repeat_unchanged() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    reduce::apply_snapshot(&mut b, limits_at(LimitStatus::Allowed, t.at(1)));
    let fx = reduce::apply_snapshot(&mut b, limits_at(LimitStatus::Allowed, t.at(2)));
    assert!(fx.is_empty());
}

// ---------------------------------------------------------------------------
// Subagent panel
// ---------------------------------------------------------------------------

#[test]
fn should_replace_the_subagent_table_wholesale_from_the_panel() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    let panel = |rows| {
        snapshot(
            t.at(1),
            SnapshotKind::Subagents {
                session: "s".into(),
                rows,
            },
        )
    };
    reduce::apply_snapshot(
        &mut b,
        panel(vec![
            sub_row("a", SubagentStatus::Running),
            sub_row("b", SubagentStatus::Failed),
        ]),
    );
    assert_eq!(truth(&b, 1), Phase::Delegating);
    assert_eq!(agent(&b, 1).subagents.source, SubagentSource::Panel);

    reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(2),
            SnapshotKind::Subagents {
                session: "s".into(),
                rows: vec![sub_row("b", SubagentStatus::Failed)],
            },
        ),
    );
    let ids: Vec<&str> = agent(&b, 1)
        .subagents
        .rows
        .keys()
        .map(|k| k.as_str())
        .collect();
    assert_eq!(ids, ["b"], "absence from the newest panel is a fact");
    assert_eq!(truth(&b, 1), Phase::Idle);
}

#[test]
fn should_drop_a_panel_taken_before_the_last_event() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(5), turn_started("p")));
    let fx = reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(4),
            SnapshotKind::Subagents {
                session: "s".into(),
                rows: vec![sub_row("zombie", SubagentStatus::Running)],
            },
        ),
    );
    assert!(fx.is_empty());
    assert!(agent(&b, 1).subagents.rows.is_empty());
}

#[test]
fn should_keep_a_gate_an_event_set_on_a_row_the_panel_still_shows() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    let panel = |at| {
        snapshot(
            t.at(at),
            SnapshotKind::Subagents {
                session: "s".into(),
                rows: vec![sub_row("a", SubagentStatus::Running)],
            },
        )
    };
    reduce::apply_snapshot(&mut b, panel(1));
    apply(
        &mut b,
        from_subagent(1, "s", "a", t.at(2), question("q", "AskUserQuestion")),
    );
    reduce::apply_snapshot(&mut b, panel(3));
    assert_eq!(truth(&b, 1), Phase::Attention);
}

/// 54 of 168 captured children started and never reported a stop. A row
/// nothing mentions for the cap is forgotten, and its expiry is a deadline
/// the core sleeps towards.
#[test]
fn should_forget_a_subagent_row_nothing_has_mentioned_for_the_stale_cap() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(
            1,
            "s",
            t.at(1),
            EventKind::SubagentStarted {
                subagent: "a1".into(),
                kind: SubagentKind::Task,
            },
        ),
    );
    assert_eq!(truth(&b, 1), Phase::Delegating);
    assert_eq!(next_deadline(&b), Some(t.at(1) + SUBAGENT_STALE));

    // Any signal from the child refreshes the row.
    apply(
        &mut b,
        from_subagent(1, "s", "a1", t.at(100), settled("c-child")),
    );
    assert_eq!(next_deadline(&b), Some(t.at(100) + SUBAGENT_STALE));

    assert!(
        tick(
            &mut b,
            t.at(100) + SUBAGENT_STALE - Duration::from_millis(1)
        )
        .is_empty()
    );
    let fx = tick(&mut b, t.at(100) + SUBAGENT_STALE);
    assert!(publishes(&fx));
    assert!(agent(&b, 1).subagents.rows.is_empty());
    assert_eq!(truth(&b, 1), Phase::Idle);
}

/// A tick never clears a live gate, so a row holding one is exempt from the
/// stale cap until the child moves again.
#[test]
fn should_keep_a_gated_subagent_row_past_the_stale_cap() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(
            1,
            "s",
            t.at(1),
            EventKind::SubagentStarted {
                subagent: "a1".into(),
                kind: SubagentKind::Task,
            },
        ),
    );
    apply(
        &mut b,
        from_subagent(1, "s", "a1", t.at(2), question("q", "AskUserQuestion")),
    );
    assert_eq!(next_deadline(&b), None, "a gated row has no expiry");
    tick(&mut b, t.at(2) + SUBAGENT_STALE * 3);
    assert_eq!(truth(&b, 1), Phase::Attention);
}

/// A panel that stopped refreshing is no longer authoritative: its rows
/// expire like any other.
#[test]
fn should_expire_panel_rows_too_when_the_panel_stops_refreshing() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(1),
            SnapshotKind::Subagents {
                session: "s".into(),
                rows: vec![sub_row("a", SubagentStatus::Running)],
            },
        ),
    );
    assert_eq!(truth(&b, 1), Phase::Delegating);
    tick(&mut b, t.at(1) + SUBAGENT_STALE);
    assert_eq!(truth(&b, 1), Phase::Idle);
}

// ---------------------------------------------------------------------------
// Waits
// ---------------------------------------------------------------------------

#[test]
fn should_wait_on_background_tasks_reported_by_stop_and_forget_them_when_expired() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    let waits = |at, ids: &[&str]| {
        snapshot(
            t.at(at),
            SnapshotKind::Waits {
                session: "s".into(),
                rows: ids
                    .iter()
                    .map(|id| WaitRow {
                        id: WaitId((*id).into()),
                        kind: WaitKind::BackgroundShell,
                    })
                    .collect(),
            },
        )
    };
    apply(&mut b, hook(1, "s", t.at(1), stop()));
    reduce::apply_snapshot(&mut b, waits(1, &["bg-1"]));
    assert_eq!(truth(&b, 1), Phase::Waiting);

    // Re-reported: `armed` survives, expiry moves.
    reduce::apply_snapshot(&mut b, waits(100, &["bg-1"]));
    let w = &agent(&b, 1).waits[&WaitId("bg-1".into())];
    assert_eq!(w.armed, t.at(1));
    assert_eq!(w.expires, t.at(100) + WAIT_EXPIRY);

    tick(&mut b, t.at(100) + WAIT_EXPIRY);
    assert_eq!(truth(&b, 1), Phase::Idle);
}

#[test]
fn should_clear_waits_when_stop_reports_none() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    let kind = |rows| SnapshotKind::Waits {
        session: "s".into(),
        rows,
    };
    reduce::apply_snapshot(
        &mut b,
        snapshot(
            t.at(1),
            kind(vec![WaitRow {
                id: WaitId("c".into()),
                kind: WaitKind::Cron,
            }]),
        ),
    );
    reduce::apply_snapshot(&mut b, snapshot(t.at(2), kind(vec![])));
    assert_eq!(truth(&b, 1), Phase::Idle);
}

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

#[test]
fn should_end_a_quiet_turn_after_the_stale_limit() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &json!({}))),
    );
    assert!(tick(&mut b, t.at(1) + TURN_STALE - Duration::from_millis(1)).is_empty());
    let fx = tick(&mut b, t.at(1) + TURN_STALE);
    assert_eq!(truth(&b, 1), Phase::Idle);
    assert!(publishes(&fx));
}

#[test]
fn should_never_end_a_turn_that_is_parked_on_a_gate() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &json!({}))),
    );
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &json!({}))));
    tick(&mut b, t.at(2) + TURN_STALE * 3);
    assert_eq!(truth(&b, 1), Phase::Attention);
}

#[test]
fn should_promote_a_pending_escalation_on_the_tick_at_its_deadline() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(&mut b, hook(1, "s", t.at(1), turn_started("p")));
    apply(
        &mut b,
        inferred(
            1,
            "s",
            t.at(2),
            turn_ended(
                None,
                TurnOutcome::Failed {
                    class: ErrorClass::Other,
                },
            ),
        ),
    );
    let deadline = next_deadline(&b).expect("a dwell is pending");
    assert_eq!(deadline, t.at(2) + Shown::INFERRED_DWELL);

    assert!(tick(&mut b, deadline - Duration::from_millis(1)).is_empty());
    let fx = tick(&mut b, deadline);
    assert_eq!(shown(&b, 1), Phase::Blocked);
    assert_eq!(history(&fx).len(), 1);
}

#[test]
fn should_report_the_earliest_deadline_across_agents_or_none() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s1"), (2, "s2")], t.at(0));
    assert_eq!(next_deadline(&b), None, "idle agents have nothing pending");
    apply(&mut b, hook(1, "s1", t.at(10), turn_started("p")));
    apply(&mut b, hook(2, "s2", t.at(3), turn_started("p")));
    assert_eq!(next_deadline(&b), Some(t.at(3) + TURN_STALE));
}

// ---------------------------------------------------------------------------
// The view (provided code: `view.rs` ships with the scaffold, so this group
// already passes)
// ---------------------------------------------------------------------------

#[test]
fn should_render_equal_views_for_an_unchanged_board() {
    let t = Clock::new();
    let b = board_with(&[(1, "s")], t.at(0));
    assert_eq!(View::render(&b), View::render(&b));
}

#[test]
fn should_explain_the_phase_in_the_tooltip() {
    let t = Clock::new();
    let mut b = board_with(&[(1, "s")], t.at(0));
    apply(
        &mut b,
        hook(1, "s", t.at(1), dispatched("c1", "Bash", &json!({}))),
    );
    apply(&mut b, hook(1, "s", t.at(2), asked("Bash", &json!({}))));
    let text = describe(&reason(agent(&b, 1), None));
    assert_eq!(text, "wants approval to run Bash");

    let view = View::render(&b);
    let v = view.to_json(t.at(3), SystemTime::UNIX_EPOCH + Duration::from_secs(1_000));
    assert_eq!(v["agents"][0]["phase"], "attention");
    assert_eq!(v["agents"][0]["lamp"], "red");
    assert_eq!(v["agents"][0]["since_ms"], 999_000);
}
