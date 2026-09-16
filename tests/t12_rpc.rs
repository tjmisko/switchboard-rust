//! T12 — the socket: one handler per connection, generic over its halves.

mod common;

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, DuplexStream, duplex};
use tokio::sync::{mpsc, watch};

use common::{FakeProc, board_with, key};
use switchboard::daemon::rpc::{handle, resolve};
use switchboard::ingest::Input;
use switchboard::model::*;
use switchboard::view::View;

struct Conn {
    to_daemon: DuplexStream,
    from_daemon: tokio::io::Lines<BufReader<DuplexStream>>,
    inbox: mpsc::Receiver<Input>,
    view: watch::Sender<Arc<View>>,
}

/// Wire `handle` to two in-memory pipes. No socket, no filesystem.
fn connect(view: View) -> Conn {
    let (client_w, server_r) = duplex(1 << 16);
    let (server_w, client_r) = duplex(1 << 16);
    let (inbox_tx, inbox) = mpsc::channel(16);
    let (view_tx, view_rx) = watch::channel(Arc::new(view));
    let root: Arc<Path> = Arc::from(Path::new("/nonexistent-proc"));
    tokio::spawn(handle(server_r, server_w, inbox_tx, view_rx, root));
    Conn {
        to_daemon: client_w,
        from_daemon: BufReader::new(client_r).lines(),
        inbox,
        view: view_tx,
    }
}

impl Conn {
    async fn ask(&mut self, line: &str) -> Value {
        self.to_daemon.write_all(line.as_bytes()).await.unwrap();
        self.to_daemon.write_all(b"\n").await.unwrap();
        self.read().await
    }

    async fn read(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(2), self.from_daemon.next_line())
            .await
            .expect("reply in time")
            .unwrap()
            .expect("a line");
        serde_json::from_str(&line).unwrap()
    }
}

#[tokio::test]
async fn should_reply_ok_and_deliver_inputs_for_a_hook_frame() {
    let mut c = connect(View { agents: vec![] });
    let reply = c
        .ask(r#"{"cmd":"hook","event":"UserPromptSubmit","session_id":"s","prompt_id":"p"}"#)
        .await;
    assert_eq!(reply, serde_json::json!({"ok": true}));
    let input = c.inbox.recv().await.unwrap();
    assert!(matches!(
        input,
        Input::Event(Event {
            kind: EventKind::TurnStarted { .. },
            process: None,
            ..
        })
    ));
}

#[tokio::test]
async fn should_reply_ok_and_deliver_nothing_for_an_untracked_event() {
    let mut c = connect(View { agents: vec![] });
    let reply = c
        .ask(r#"{"cmd":"hook","event":"Notification","session_id":"s"}"#)
        .await;
    assert_eq!(reply["ok"], true);
    assert!(c.inbox.try_recv().is_err());
}

#[tokio::test]
async fn should_reply_with_an_error_and_keep_serving_after_a_bad_line() {
    let mut c = connect(View { agents: vec![] });
    let bad = c.ask("this is not json").await;
    assert_eq!(bad["ok"], false);
    assert!(bad["error"].as_str().unwrap().starts_with("malformed JSON"));

    let unknown = c.ask(r#"{"cmd":"nope"}"#).await;
    assert_eq!(unknown["error"], "unknown command: nope");

    let list = c.ask(r#"{"cmd":"list"}"#).await;
    assert!(list["snapshot"]["agents"].is_array());
}

#[tokio::test]
async fn should_list_the_current_view() {
    let t = common::Clock::new();
    let board = board_with(&[(42, "s42")], t.at(0));
    let mut c = connect(View::render(&board));
    let reply = c.ask(r#"{"cmd":"list"}"#).await;
    let agents = reply["snapshot"]["agents"].as_array().unwrap();
    assert_eq!(agents.len(), 1);
    assert_eq!(agents[0]["pid"], 42);
    assert_eq!(agents[0]["phase"], "idle");
}

#[tokio::test]
async fn should_stream_a_snapshot_now_and_again_on_each_change_when_subscribed() {
    let t = common::Clock::new();
    let mut c = connect(View { agents: vec![] });
    let first = c.ask(r#"{"cmd":"subscribe"}"#).await;
    assert_eq!(first["snapshot"]["agents"].as_array().unwrap().len(), 0);

    let board = board_with(&[(1, "s")], t.at(0));
    c.view.send(Arc::new(View::render(&board))).unwrap();
    let second = c.read().await;
    assert_eq!(second["snapshot"]["agents"][0]["pid"], 1);
}

#[tokio::test]
async fn should_ignore_bytes_beyond_the_connection_cap() {
    let mut c = connect(View { agents: vec![] });
    let huge = format!(
        r#"{{"cmd":"list","pad":"{}"}}"#,
        "x".repeat(switchboard::daemon::rpc::MAX_CONNECTION_BYTES as usize)
    );
    // The handler stops reading at the cap: `Take` reports EOF, the truncated
    // line fails to decode, `next_line` then yields `None` and the handler
    // returns, dropping both halves. So the tail of this write may fail with
    // a broken pipe. That is the point.
    let _ = c.to_daemon.write_all(huge.as_bytes()).await;
    let _ = c.to_daemon.write_all(b"\n").await;
    // Whatever it got was not a complete request, so the connection ends
    // without a snapshot.
    let next = tokio::time::timeout(Duration::from_secs(2), c.from_daemon.next_line()).await;
    match next {
        Ok(Ok(Some(line))) => assert!(!line.contains("snapshot"), "{line}"),
        Ok(Ok(None)) | Ok(Err(_)) => {}
        Err(_) => panic!("handler hung"),
    }
}

#[tokio::test]
async fn should_resolve_a_hook_pid_to_the_nearest_tracked_ancestor() {
    let root = common::scratch_dir("rpc_resolve");
    let fake = FakeProc { root: &root };
    let cwd = Path::new("/");
    fake.add(50, "claude", 1, 1000, &["claude"], cwd);
    fake.add(60, "sh", 50, 1001, &["sh"], cwd);
    fake.add(70, "switchboard-hoo", 60, 1002, &["switchboard-hook"], cwd);

    let t = common::Clock::new();
    let board = board_with(&[(50, "s")], t.at(0));
    let (_tx, rx) = watch::channel(Arc::new(View::render(&board)));
    let root: Arc<Path> = Arc::from(root.as_path());

    assert_eq!(resolve(&root, Some(70), &rx).await, Some(key(50)));
    assert_eq!(resolve(&root, Some(50), &rx).await, Some(key(50)));
    assert_eq!(resolve(&root, None, &rx).await, None);
    assert_eq!(resolve(&root, Some(9999), &rx).await, None);
}

#[tokio::test]
async fn should_serve_a_real_unix_socket_end_to_end() {
    let dir = std::env::temp_dir().join(format!("sbr-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t11.sock");

    let listener = switchboard::daemon::bind(&path).await.unwrap();
    let mode =
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions());
    assert_eq!(mode & 0o777, 0o600);

    let (inbox_tx, mut inbox) = mpsc::channel(4);
    let (_view_tx, view_rx) = watch::channel(Arc::new(View { agents: vec![] }));
    let server = tokio::spawn(switchboard::daemon::rpc::serve(
        listener,
        inbox_tx,
        view_rx,
        "/nonexistent".into(),
    ));

    // A second daemon must refuse to steal the socket.
    assert!(switchboard::daemon::bind(&path).await.is_err());

    // The synchronous forwarder client, on the blocking pool.
    let p = path.clone();
    let sent = tokio::task::spawn_blocking(move || {
        switchboard::client::send(
            &p,
            &serde_json::json!({"cmd":"hook","event":"Stop","session_id":"s"}),
            Duration::from_secs(1),
        )
    })
    .await
    .unwrap();
    assert!(sent.is_ok(), "{sent:?}");
    assert!(matches!(inbox.recv().await, Some(Input::Event(_))));

    server.abort();
    let _ = std::fs::remove_file(&path);
}
