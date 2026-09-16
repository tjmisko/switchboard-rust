//! T14 — the second pass: a subscriber that notices a hang-up, and a watcher
//! that stops on a signal. Both are changes to T12 and T13 code; `t12_rpc`
//! and `t13_tasks` must keep passing unchanged.
//!
//! The watcher tests run on real time, like `t13_tasks`, because each pass
//! goes through the blocking pool. Where a test needs "before the next tick"
//! to mean something, the poll interval is ten seconds and the timeout one.

mod common;

use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, Lines, duplex};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use common::{board_with, key};
use switchboard::daemon::rpc::handle;
use switchboard::daemon::watcher::{self, WatchSpec};
use switchboard::ingest::Input;
use switchboard::model::*;
use switchboard::reduce::default_account;
use switchboard::view::View;

// ---------------------------------------------------------------------------
// Part A: the subscriber
// ---------------------------------------------------------------------------

struct Conn {
    to_daemon: DuplexStream,
    from_daemon: Lines<BufReader<DuplexStream>>,
    view: watch::Sender<Arc<View>>,
    /// The `handle` task itself: these tests watch it end.
    task: JoinHandle<()>,
    _inbox: mpsc::Receiver<Input>,
}

/// `t12_rpc`'s `connect`, keeping the handler's `JoinHandle`.
fn connect(view: View) -> Conn {
    let (client_w, server_r) = duplex(1 << 16);
    let (server_w, client_r) = duplex(1 << 16);
    let (inbox_tx, inbox) = mpsc::channel(16);
    let (view_tx, view_rx) = watch::channel(Arc::new(view));
    let root: Arc<Path> = Arc::from(Path::new("/nonexistent-proc"));
    let task = tokio::spawn(handle(server_r, server_w, inbox_tx, view_rx, root));
    Conn {
        to_daemon: client_w,
        from_daemon: BufReader::new(client_r).lines(),
        view: view_tx,
        task,
        _inbox: inbox,
    }
}

impl Conn {
    async fn send(&mut self, line: &str) {
        self.to_daemon.write_all(line.as_bytes()).await.unwrap();
        self.to_daemon.write_all(b"\n").await.unwrap();
    }

    async fn subscribe(&mut self) -> Value {
        self.send(r#"{"cmd":"subscribe"}"#).await;
        self.read().await
    }

    async fn read(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(2), self.from_daemon.next_line())
            .await
            .expect("a line in time")
            .unwrap()
            .expect("a line");
        serde_json::from_str(&line).unwrap()
    }

    /// Nothing arrives for a while. Long enough that anything the handler
    /// wrote in response to the last send would have crossed the pipe.
    async fn expect_quiet(&mut self) {
        let next =
            tokio::time::timeout(Duration::from_millis(200), self.from_daemon.next_line()).await;
        assert!(next.is_err(), "expected no line, got {next:?}");
    }

    /// The handler task ends promptly.
    async fn expect_ended(self, why: &str) {
        tokio::time::timeout(Duration::from_secs(1), self.task)
            .await
            .expect(why)
            .unwrap();
    }
}

#[tokio::test]
async fn should_end_the_handler_when_the_subscriber_hangs_up_while_the_view_is_quiet() {
    let mut c = connect(View { agents: vec![] });
    c.subscribe().await;

    // The peer goes away, and the view never changes again. A handler that
    // only waits on `changed()` sits here until the sender drops, which in
    // the daemon is never.
    let hung_up = std::mem::replace(&mut c.to_daemon, duplex(1).0);
    drop(hung_up);
    c.expect_ended("handler ends without a view change").await;
}

#[tokio::test]
async fn should_ignore_a_further_line_and_still_deliver_the_next_change_when_subscribed() {
    let t = common::Clock::new();
    let mut c = connect(View { agents: vec![] });
    let first = c.subscribe().await;
    assert_eq!(first["snapshot"]["agents"].as_array().unwrap().len(), 0);

    // A request after `subscribe` is neither answered nor a reason to
    // resend the snapshot.
    c.send(r#"{"cmd":"list"}"#).await;
    c.expect_quiet().await;

    let board = board_with(&[(1, "s")], t.at(0));
    c.view.send(Arc::new(View::render(&board))).unwrap();
    let second = c.read().await;
    assert_eq!(second["snapshot"]["agents"][0]["pid"], 1);
}

#[tokio::test]
async fn should_end_the_handler_when_the_subscriber_hangs_up_after_a_further_line() {
    let mut c = connect(View { agents: vec![] });
    c.subscribe().await;
    c.send("ignored").await;
    c.expect_quiet().await;

    let hung_up = std::mem::replace(&mut c.to_daemon, duplex(1).0);
    drop(hung_up);
    c.expect_ended("handler ends after an ignored line and a hang-up").await;
}

#[tokio::test]
async fn should_end_the_handler_when_the_view_sender_is_dropped_while_subscribed() {
    let mut c = connect(View { agents: vec![] });
    c.subscribe().await;
    // T12's exit is still an exit: the daemon's view is gone.
    let gone = std::mem::replace(&mut c.view, watch::channel(Arc::new(View { agents: vec![] })).0);
    drop(gone);
    c.expect_ended("handler ends when the view's sender is dropped").await;
}

// ---------------------------------------------------------------------------
// Part B: the watcher
// ---------------------------------------------------------------------------

const PROMPT: &str =
    r#"{"type":"user","promptId":"p1","message":{"content":"hi"},"origin":{"kind":"human"}}"#;
const TOOL: &str = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"c1","name":"Bash","input":{}}]}}"#;

/// A poll interval no test waits out: a watcher that ends "before the next
/// tick" ends in the one-second timeout, one that waits for the tick does
/// not.
const LONG_POLL: Duration = Duration::from_secs(10);

fn spec(path: std::path::PathBuf, poll: Duration) -> WatchSpec {
    WatchSpec {
        path,
        process: key(1),
        session: "s".into(),
        account: default_account(),
        poll,
        replay_bytes: 1 << 20,
    }
}

/// A transcript with one prompt in it, in a fresh directory.
fn transcript(name: &str) -> std::path::PathBuf {
    let path = common::scratch_dir(name).join("s.jsonl");
    std::fs::write(&path, format!("{PROMPT}\n")).unwrap();
    path
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

async fn expect_ended(task: JoinHandle<()>, why: &str) {
    tokio::time::timeout(Duration::from_secs(1), task)
        .await
        .expect(why)
        .unwrap();
}

#[tokio::test]
async fn should_end_before_the_next_tick_when_the_stop_is_signalled() {
    let path = transcript("watcher_stop_signalled");
    let (tx, mut inbox) = mpsc::channel(16);
    let (stop, stop_rx) = watch::channel(false);
    let task = tokio::spawn(watcher::run_until(spec(path, LONG_POLL), tx, stop_rx));

    // The replay has arrived: the watcher is following, and its next tick
    // is ten seconds away.
    recv(&mut inbox).await;
    stop.send(true).unwrap();
    expect_ended(task, "watcher ends on the signal, not on the next tick").await;
}

#[tokio::test]
async fn should_end_before_the_next_tick_when_the_stop_sender_is_dropped() {
    let path = transcript("watcher_stop_dropped");
    let (tx, mut inbox) = mpsc::channel(16);
    let (stop, stop_rx) = watch::channel(false);
    let task = tokio::spawn(watcher::run_until(spec(path, LONG_POLL), tx, stop_rx));

    recv(&mut inbox).await;
    // Whoever held the sender is gone. That is a stop too.
    drop(stop);
    expect_ended(task, "watcher ends when the stop sender is dropped").await;
}

#[tokio::test]
async fn should_end_promptly_when_the_stop_was_signalled_before_the_first_pass() {
    let path = transcript("watcher_stop_early");
    let (tx, _inbox) = mpsc::channel(16);
    // Already `true` when the watcher starts. `changed()` would sleep
    // through this: the initial value counts as seen.
    let (_stop, stop_rx) = watch::channel(true);
    let task = tokio::spawn(watcher::run_until(spec(path, LONG_POLL), tx, stop_rx));
    expect_ended(task, "watcher ends on a stop raised before it started").await;
}

#[tokio::test]
async fn should_deliver_what_the_last_poll_read_before_ending_when_stopped() {
    let path = transcript("watcher_stop_delivers");
    let (tx, mut inbox) = mpsc::channel(16);
    let (stop, stop_rx) = watch::channel(false);
    let task = tokio::spawn(watcher::run_until(
        spec(path.clone(), Duration::from_millis(10)),
        tx,
        stop_rx,
    ));

    let replayed = recv(&mut inbox).await;
    assert_eq!(replayed.confidence, Confidence::Inferred);

    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{TOOL}").unwrap();
    let live = recv(&mut inbox).await;
    assert_eq!(live.confidence, Confidence::Confirmed);
    assert!(matches!(live.kind, EventKind::ToolDispatched { .. }));

    // Everything read so far has been sent. Now stop: the watcher ends and,
    // with it, its end of the inbox.
    stop.send(true).unwrap();
    expect_ended(task, "watcher ends after delivering what it read").await;
    assert!(inbox.recv().await.is_none(), "nothing after the stop");
}

#[tokio::test]
async fn should_keep_following_when_run_holds_its_own_stop() {
    // `run` delegates to `run_until` with a stop it never raises. The sender
    // has to live as long as `run` does: bound to `_` it is dropped at the
    // end of its `let`, and a dropped sender is a stop.
    let path = transcript("watcher_run_holds_stop");
    let (tx, mut inbox) = mpsc::channel(16);
    let task = tokio::spawn(watcher::run(spec(path.clone(), Duration::from_millis(10)), tx));

    recv(&mut inbox).await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(!task.is_finished(), "run ended with nobody asking it to");

    let mut f = std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap();
    writeln!(f, "{TOOL}").unwrap();
    let live = recv(&mut inbox).await;
    assert!(matches!(live.kind, EventKind::ToolDispatched { .. }));
    task.abort();
}
