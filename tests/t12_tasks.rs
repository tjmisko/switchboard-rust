//! T12 — tasks that own things: watchers, the effect runner, the scanner diff.

mod common;

use std::collections::BTreeSet;
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::mpsc;

use common::{key, row};
use switchboard::daemon::effects::Watchers;
use switchboard::daemon::publish::write_atomic;
use switchboard::daemon::scanner::sweep_inputs;
use switchboard::daemon::watcher::{self, WatchSpec};
use switchboard::ingest::Input;
use switchboard::model::*;
use switchboard::reduce::default_account;

fn spec(path: std::path::PathBuf, replay_bytes: u64) -> WatchSpec {
    WatchSpec {
        path,
        process: key(1),
        session: "s".into(),
        account: default_account(),
        poll: Duration::from_millis(10),
        replay_bytes,
    }
}

async fn recv(inbox: &mut mpsc::Receiver<Input>) -> Event {
    let input = tokio::time::timeout(Duration::from_secs(2), inbox.recv())
        .await
        .expect("an input in time")
        .expect("channel open");
    match input {
        Input::Event(e) => e,
        Input::Snapshot(s) => panic!("unexpected snapshot {s:?}"),
    }
}

const PROMPT: &str = r#"{"type":"user","promptId":"p1","message":{"content":"hi"},"origin":{"kind":"human"}}"#;
const TOOL: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"c1","name":"Bash","input":{}}]}}"#;

#[tokio::test]
async fn should_replay_the_tail_as_inferred_then_follow_new_lines_as_confirmed() {
    let dir = common::scratch_dir("watcher_replay");
    let path = dir.join("s.jsonl");
    std::fs::write(&path, format!("{PROMPT}\n")).unwrap();

    let (tx, mut inbox) = mpsc::channel(16);
    let task = tokio::spawn(watcher::run(spec(path.clone(), 1 << 20), tx));

    let replayed = recv(&mut inbox).await;
    assert_eq!(replayed.confidence, Confidence::Inferred);
    assert!(matches!(replayed.kind, EventKind::TurnStarted { .. }));
    assert_eq!(replayed.process, Some(key(1)));

    let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
    writeln!(f, "{TOOL}").unwrap();
    let live = recv(&mut inbox).await;
    assert_eq!(live.confidence, Confidence::Confirmed);
    assert!(matches!(live.kind, EventKind::ToolDispatched { .. }));

    task.abort();
}

#[tokio::test]
async fn should_wait_for_a_transcript_that_does_not_exist_yet() {
    let dir = common::scratch_dir("watcher_absent");
    let path = dir.join("later.jsonl");
    let (tx, mut inbox) = mpsc::channel(16);
    let task = tokio::spawn(watcher::run(spec(path.clone(), 1 << 20), tx));

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(inbox.try_recv().is_err());
    std::fs::write(&path, format!("{PROMPT}\n")).unwrap();
    let first = recv(&mut inbox).await;
    assert!(matches!(first.kind, EventKind::TurnStarted { .. }));
    task.abort();
}

#[tokio::test]
async fn should_replay_only_the_window_at_the_end_of_a_long_transcript() {
    let dir = common::scratch_dir("watcher_window");
    let path = dir.join("s.jsonl");
    let mut body = String::new();
    for _ in 0..50 {
        body.push_str(PROMPT);
        body.push('\n');
    }
    body.push_str(TOOL);
    body.push('\n');
    std::fs::write(&path, &body).unwrap();

    let (tx, mut inbox) = mpsc::channel(128);
    let task = tokio::spawn(watcher::run(spec(path, TOOL.len() as u64 + 1), tx));
    let only = recv(&mut inbox).await;
    assert!(matches!(only.kind, EventKind::ToolDispatched { .. }));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(inbox.try_recv().is_err(), "nothing before the window");
    task.abort();
}

#[tokio::test]
async fn should_stop_when_the_inbox_closes() {
    let dir = common::scratch_dir("watcher_close");
    let path = dir.join("s.jsonl");
    std::fs::write(&path, format!("{PROMPT}\n")).unwrap();
    let (tx, inbox) = mpsc::channel(1);
    drop(inbox);
    let task = tokio::spawn(watcher::run(spec(path, 1 << 20), tx));
    tokio::time::timeout(Duration::from_secs(2), task)
        .await
        .expect("watcher exits once nobody is listening")
        .unwrap();
}

/// A task that records whether it was dropped before finishing.
fn sentinel() -> (tokio::task::JoinHandle<()>, Arc<AtomicBool>) {
    struct SetOnDrop(Arc<AtomicBool>);
    impl Drop for SetOnDrop {
        fn drop(&mut self) {
            self.0.store(true, Ordering::SeqCst);
        }
    }
    let flag = Arc::new(AtomicBool::new(false));
    let guard = SetOnDrop(flag.clone());
    let handle = tokio::spawn(async move {
        let _guard = guard;
        std::future::pending::<()>().await;
    });
    (handle, flag)
}

async fn settle() {
    for _ in 0..10 {
        tokio::task::yield_now().await;
    }
}

#[tokio::test]
async fn should_abort_the_previous_watcher_when_a_process_gets_a_new_one() {
    let mut watchers = Watchers::default();
    let (first, first_dropped) = sentinel();
    let (second, second_dropped) = sentinel();
    watchers.start(key(1), first);
    watchers.start(key(1), second);
    settle().await;
    assert!(first_dropped.load(Ordering::SeqCst));
    assert!(!second_dropped.load(Ordering::SeqCst));
    assert_eq!(watchers.len(), 1);
}

#[tokio::test]
async fn should_abort_on_stop_and_abort_everything_left_on_drop() {
    let mut watchers = Watchers::default();
    let (a, a_dropped) = sentinel();
    let (b, b_dropped) = sentinel();
    watchers.start(key(1), a);
    watchers.start(key(2), b);
    watchers.stop(&key(1));
    watchers.stop(&key(99));
    settle().await;
    assert!(a_dropped.load(Ordering::SeqCst));
    assert!(!b_dropped.load(Ordering::SeqCst));

    drop(watchers);
    settle().await;
    assert!(b_dropped.load(Ordering::SeqCst));
}

#[test]
fn should_emit_the_sweep_then_a_death_for_each_vanished_process() {
    let now = Instant::now();
    let known: BTreeSet<ProcKey> = [key(1), key(2), key(3)].into();
    let (inputs, seen) = sweep_inputs(&known, vec![row(2, None), row(4, None)], now);

    assert_eq!(seen, [key(2), key(4)].into());
    assert!(matches!(
        &inputs[0],
        Input::Snapshot(Snapshot { kind: SnapshotKind::Processes(rows), .. }) if rows.len() == 2
    ));
    let died: Vec<u32> = inputs[1..]
        .iter()
        .map(|i| match i {
            Input::Event(Event {
                kind: EventKind::Died,
                process: Some(p),
                ..
            }) => p.pid,
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(died, [1, 3]);
}

#[test]
fn should_treat_a_recycled_pid_as_a_death_and_a_birth() {
    let now = Instant::now();
    let old = ProcKey { pid: 5, start: 100 };
    let new = ProcKey { pid: 5, start: 200 };
    let mut reborn = row(5, None);
    reborn.key = new;
    let (inputs, seen) = sweep_inputs(&[old].into(), vec![reborn], now);
    assert_eq!(seen, [new].into());
    assert_eq!(inputs.len(), 2);
}

#[tokio::test]
async fn should_replace_a_file_atomically_and_leave_no_temp_behind() {
    let dir = common::scratch_dir("publish_atomic");
    let path = dir.join("nested/state.json");
    write_atomic(&path, b"{\"v\":1}").await.unwrap();
    write_atomic(&path, b"{\"v\":2}").await.unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"{\"v\":2}");
    let names: Vec<String> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(names, ["state.json"]);
}
