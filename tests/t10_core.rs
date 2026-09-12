//! T10 — the core task: one owner, two wake-ups, paused time.
//!
//! `start_paused = true` freezes tokio's clock. When every task is idle the
//! runtime jumps the clock to the next timer, so a ten-second dwell takes no
//! real time — and `switchboard::daemon::now()` reads that same clock.

mod common;

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use common::{key, row};
use switchboard::daemon::{core, now};
use switchboard::ingest::Input;
use switchboard::model::*;
use switchboard::view::View;

struct Harness {
    inbox: mpsc::Sender<Input>,
    view: watch::Receiver<Arc<View>>,
    effects: mpsc::UnboundedReceiver<Effect>,
    task: JoinHandle<Board>,
}

fn start() -> Harness {
    let (inbox, inbox_rx) = mpsc::channel(64);
    let (view_tx, view) = watch::channel(Arc::new(View::render(&Board::default())));
    let (effects_tx, effects) = mpsc::unbounded_channel();
    let task = tokio::spawn(core::run_until_closed(
        Board::default(),
        inbox_rx,
        view_tx,
        effects_tx,
    ));
    Harness {
        inbox,
        view,
        effects,
        task,
    }
}

fn discover(pid: u32, session: &str) -> Input {
    Input::Snapshot(Snapshot {
        taken: now(),
        kind: SnapshotKind::Processes(vec![row(pid, Some(session))]),
    })
}

fn event(pid: u32, session: &str, cause: Cause, confidence: Confidence, kind: EventKind) -> Input {
    Input::Event(Event {
        at: Moment {
            local: now(),
            claimed: None,
        },
        process: Some(key(pid)),
        session: session.into(),
        actor: Actor::Agent,
        cause,
        confidence,
        kind,
    })
}

fn start_session(pid: u32, session: &str) -> Input {
    event(
        pid,
        session,
        Cause::Agent,
        Confidence::Confirmed,
        EventKind::SessionStarted {
            source: StartSource::Startup,
            title: None,
        },
    )
}

fn turn(pid: u32, session: &str, prompt: &str) -> Input {
    event(
        pid,
        session,
        Cause::User,
        Confidence::Confirmed,
        common::turn_started(prompt),
    )
}

fn shown_phase(view: &watch::Receiver<Arc<View>>, pid: u32) -> Option<Phase> {
    view.borrow()
        .agents
        .iter()
        .find(|a| a.process.pid == pid)
        .map(|a| a.shown)
}

#[tokio::test(start_paused = true)]
async fn should_publish_a_new_view_when_an_input_changes_the_board() {
    let mut h = start();
    h.inbox.send(discover(1, "s")).await.unwrap();
    h.view.changed().await.unwrap();
    assert_eq!(shown_phase(&h.view, 1), Some(Phase::Unknown));

    h.inbox.send(start_session(1, "s")).await.unwrap();
    h.view.changed().await.unwrap();
    assert_eq!(shown_phase(&h.view, 1), Some(Phase::Idle));
}

#[tokio::test(start_paused = true)]
async fn should_forward_every_effect_except_publish_to_the_runner() {
    let mut h = start();
    h.inbox.send(discover(7, "s7")).await.unwrap();
    drop(h.inbox);
    h.task.await.unwrap();

    let mut forwarded = Vec::new();
    while let Ok(e) = h.effects.try_recv() {
        forwarded.push(e);
    }
    assert!(forwarded.iter().all(|e| *e != Effect::PublishState));
    assert!(
        forwarded
            .iter()
            .any(|e| matches!(e, Effect::WatchTranscript { process, .. } if process.pid == 7))
    );
    assert!(
        forwarded
            .iter()
            .any(|e| matches!(e, Effect::AppendHistory(HistoryEntry::AgentFound { .. })))
    );
}

#[tokio::test(start_paused = true)]
async fn should_not_wake_subscribers_when_an_input_changes_nothing_visible() {
    let mut h = start();
    h.inbox.send(discover(1, "s")).await.unwrap();
    h.inbox.send(start_session(1, "s")).await.unwrap();
    h.inbox.send(turn(1, "s", "p")).await.unwrap();
    // Wait until the core has caught up with all three.
    while shown_phase(&h.view, 1) != Some(Phase::Working) {
        h.view.changed().await.unwrap();
    }
    h.view.borrow_and_update();

    h.inbox.send(turn(1, "s", "p")).await.unwrap(); // duplicate: a no-op
    drop(h.inbox);
    h.task.await.unwrap();
    assert!(!h.view.has_changed().unwrap_or(false));
}

#[tokio::test(start_paused = true)]
async fn should_promote_a_dwell_with_no_further_input() {
    let h = start();
    h.inbox.send(discover(1, "s")).await.unwrap();
    h.inbox.send(start_session(1, "s")).await.unwrap();
    h.inbox.send(turn(1, "s", "p")).await.unwrap();
    h.inbox
        .send(event(
            1,
            "s",
            Cause::Agent,
            Confidence::Inferred,
            common::turn_ended(
                None,
                TurnOutcome::Failed {
                    class: ErrorClass::RateLimit,
                },
            ),
        ))
        .await
        .unwrap();

    tokio::time::sleep(Duration::from_secs(5)).await;
    assert_eq!(shown_phase(&h.view, 1), Some(Phase::Working), "mid-dwell");

    // The core's own deadline wakes it; nothing else is sent.
    tokio::time::sleep(Shown::INFERRED_DWELL).await;
    assert_eq!(shown_phase(&h.view, 1), Some(Phase::Blocked));
    drop(h.inbox);
}

#[tokio::test(start_paused = true)]
async fn should_end_a_stale_turn_on_its_own_deadline() {
    let h = start();
    h.inbox.send(discover(1, "s")).await.unwrap();
    h.inbox.send(start_session(1, "s")).await.unwrap();
    h.inbox.send(turn(1, "s", "p")).await.unwrap();

    tokio::time::sleep(switchboard::reduce::TURN_STALE + Duration::from_secs(1)).await;
    assert_eq!(shown_phase(&h.view, 1), Some(Phase::Idle));
}

#[tokio::test(start_paused = true)]
async fn should_hand_back_the_board_when_every_sender_is_dropped() {
    let h = start();
    let second = h.inbox.clone();
    h.inbox.send(discover(1, "s")).await.unwrap();
    drop(h.inbox);
    second.send(discover(2, "t")).await.unwrap();
    drop(second);

    let board = h.task.await.unwrap();
    assert_eq!(board.agents.len(), 2);
}
