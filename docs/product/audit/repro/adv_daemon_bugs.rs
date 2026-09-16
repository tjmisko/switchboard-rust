//! Adversarial audit: daemon-level minimal reproductions. Each test asserts
//! the behaviour the golden should have and fails today.
//!
//! Run: `cargo test --test adv_daemon_bugs`
//! The live-binary versions are `tests/adv_daemon_fuzz.py emfile` and
//! `tests/adv_cascade.py 4100`.

mod common;

use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::json;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, duplex};
use tokio::sync::{mpsc, watch};

use common::*;
use switchboard::daemon::rpc::handle;
use switchboard::daemon::scanner::sweep_inputs;
use switchboard::ingest::Input;
use switchboard::ingest::claude::{LineCtx, line};
use switchboard::model::*;
use switchboard::proc::sweep;
use switchboard::reduce::{self, default_account};
use switchboard::view::View;

// ---------------------------------------------------------------------------
// BUG D1 (High): one failed sweep declares every agent dead.
// ---------------------------------------------------------------------------

/// `proc::sweep` returns an empty Vec when `read_dir(/proc)` fails (and drops
/// any pid whose `stat` cannot be opened), and `sweep_inputs` turns every
/// absence into `Died`. Under fd exhaustion (seen live: EMFILE with nofile=64)
/// every agent is forgotten, its watcher aborted and a `session_end` written,
/// then rediscovered and its transcript replayed a second later.
#[test]
fn should_not_declare_every_agent_dead_when_proc_cannot_be_read() {
    let known: BTreeSet<ProcKey> = [key(1), key(2)].into();
    let rows = sweep(Path::new("/nonexistent-proc-root"), Path::new("/nonexistent"));
    let (inputs, next_known) = sweep_inputs(&known, rows, Instant::now());
    let deaths = inputs
        .iter()
        .filter(|i| matches!(i, Input::Event(Event { kind: EventKind::Died, .. })))
        .count();
    assert_eq!(deaths, 0, "an unreadable /proc is not evidence of death");
    assert_eq!(next_known, known);
}

// ---------------------------------------------------------------------------
// BUG D2 (High): the forwarder can block forever in connect().
// ---------------------------------------------------------------------------

/// `client::send` sets its timeouts *after* `UnixStream::connect`, which has
/// none. When the daemon is not accepting (SIGSTOP, or the EMFILE accept spin
/// in D3) the backlog fills and every hook blocks until Claude's hook timeout
/// kills it, stalling the agent it was meant to observe.
#[tokio::test(flavor = "multi_thread")]
async fn should_return_within_the_hook_budget_when_the_daemon_is_not_accepting() {
    let dir = scratch_dir("adv_fwd_backlog");
    let path = dir.join("s.sock");
    let _listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let mut held = Vec::new();
    while let Ok(s) = tokio::net::UnixStream::connect(&path).await {
        held.push(s);
        assert!(held.len() < 100_000, "backlog never filled");
    }
    let p = path.clone();
    let started = Instant::now();
    let sender = std::thread::spawn(move || {
        switchboard::client::send(&p, &json!({"cmd": "hook"}), Duration::from_millis(500))
    });
    while !sender.is_finished() && started.elapsed() < Duration::from_secs(3) {
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        sender.is_finished(),
        "client::send still blocked in connect() after 3 s (budget is 500 ms); backlog held {}",
        held.len()
    );
}

// ---------------------------------------------------------------------------
// BUG D3 (High) is live-only: rpc.rs `let Ok(..) = listener.accept().await
// else { continue; }` spins at 100% CPU on EMFILE and stops answering.
// See tests/adv_daemon_fuzz.py emfile.
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// BUG D4 (Low): a non-UTF-8 line kills the connection with no reply.
// ---------------------------------------------------------------------------

#[tokio::test]
async fn should_reply_with_an_error_and_keep_serving_after_a_non_utf8_line() {
    let t = Clock::new();
    let board = board_with(&[(1, "s")], t.at(0));
    let (client_w, server_r) = duplex(1 << 16);
    let (server_w, client_r) = duplex(1 << 16);
    let (inbox_tx, _inbox) = mpsc::channel(16);
    let (_view_tx, view_rx) = watch::channel(Arc::new(View::render(&board)));
    let root: Arc<Path> = Arc::from(Path::new("/nonexistent"));
    tokio::spawn(handle(server_r, server_w, inbox_tx, view_rx, root));

    let mut w = client_w;
    let mut lines = BufReader::new(client_r).lines();
    w.write_all(b"\xff\xfe\n{\"cmd\":\"list\"}\n").await.unwrap();
    let first = tokio::time::timeout(Duration::from_secs(1), lines.next_line()).await;
    let first = first.expect("no hang").unwrap();
    assert!(
        first.as_deref().is_some_and(|l| l.contains("\"ok\":false")),
        "expected an error reply, got {first:?} (connection closed)"
    );
}

// ---------------------------------------------------------------------------
// BUG D5 (Low): a connection that never finishes a line is held forever.
// ---------------------------------------------------------------------------

/// No read timeout: 5000 such connections were held live (fds 12 -> 5012,
/// RSS 15 -> 55 MiB). With a systemd user unit's default soft NOFILE of
/// 1024, ~1000 of them reach D3.
#[tokio::test(start_paused = true)]
async fn should_drop_a_connection_that_never_completes_a_line() {
    let t = Clock::new();
    let board = board_with(&[(1, "s")], t.at(0));
    let (mut client_w, server_r) = duplex(1 << 16);
    let (server_w, _client_r) = duplex(1 << 16);
    let (inbox_tx, _inbox) = mpsc::channel(16);
    let (_view_tx, view_rx) = watch::channel(Arc::new(View::render(&board)));
    let root: Arc<Path> = Arc::from(Path::new("/nonexistent"));
    client_w.write_all(b"{\"cmd\":\"hook\"").await.unwrap();
    let served = tokio::spawn(handle(server_r, server_w, inbox_tx, view_rx, root));
    let finished = tokio::time::timeout(Duration::from_secs(3600), served).await;
    assert!(finished.is_ok(), "handler still holding a half-line connection after an hour");
    drop(client_w);
}

// ---------------------------------------------------------------------------
// BUG D6 (Low-Medium): replay writes history for turns that ended long ago.
// ---------------------------------------------------------------------------

/// Every discovery or `/clear` replays up to 128 KiB. Each replayed turn is
/// settled separately and `Working <-> Idle` is free in both directions, so
/// history gets a burst of transitions stamped *now*. Seen live: six `shown`
/// records for pid 6002 in the same second at every rediscovery.
#[test]
fn should_not_append_history_for_each_turn_of_a_replayed_transcript() {
    let t = Clock::new();
    let mut b = board_unobserved(&[(100, Some("s1"))], t.at(0));
    let ctx = LineCtx {
        process: key(100),
        session: SessionId::from("s1"),
        account: default_account(),
        now: t.at(1),
        confidence: Confidence::Inferred,
    };
    let mut shown = 0;
    for p in ["p1", "p2", "p3"] {
        let prompt = json!({"type": "user", "promptId": p, "message": {"content": "go"}});
        let done = json!({"type": "assistant", "message": {"stop_reason": "end_turn", "content": []}});
        for v in [prompt, done] {
            for input in line(&v, &ctx) {
                shown += reduce::apply(&mut b, input)
                    .iter()
                    .filter(|e| matches!(e, Effect::AppendHistory(HistoryEntry::Shown { .. })))
                    .count();
            }
        }
    }
    assert!(shown <= 1, "one replay batch appended {shown} shown records");
}
