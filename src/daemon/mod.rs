//! The running daemon: tasks, channels, and the one place with tokio.
//!
//! ```text
//! rpc listener ─┐                                   ┌─> publisher ─> state.json
//! scanner ──────┼─ mpsc<Input> ─> core ─ watch<View>┼─> rpc subscribers
//! watchers[n] ──┘                  │                └
//!                                  └─ mpsc<Effect> ─> effect runner ─┬─> watchers[n]
//!                                                                    └─> history appender
//! ```
//!
//! The core is the only owner of the `Board`. Everything else talks to it
//! through channels: inputs in, a published view out, effects out. Effects
//! that learn something come back round as new inputs.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::UnixListener;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{mpsc, watch};
use tokio::task::JoinSet;

use crate::error::BoxError;
use crate::model::Board;
use crate::paths::Layout;
use crate::view::View;

pub mod core;
pub mod effects;
pub mod history;
pub mod publish;
pub mod rpc;
pub mod scanner;
pub mod watcher;

/// The daemon's clock. Reads tokio's, so a test with paused time controls
/// every timestamp the reducer sees, and converts to the `std` type the model
/// uses.
pub fn now() -> std::time::Instant {
    tokio::time::Instant::now().into_std()
}

/// Tunables that are not paths.
#[derive(Debug, Clone)]
pub struct Settings {
    pub scan_every: Duration,
    pub transcript_poll: Duration,
    /// How much of a transcript's end to replay on discovery.
    pub replay_bytes: u64,
    /// How long the publisher waits for more changes before writing (R6).
    pub coalesce: Duration,
    pub inbox_capacity: usize,
    pub history_capacity: usize,
}

impl Default for Settings {
    fn default() -> Settings {
        Settings {
            scan_every: Duration::from_secs(1),
            transcript_poll: Duration::from_millis(200),
            replay_bytes: 128 * 1024,
            coalesce: Duration::from_millis(250),
            inbox_capacity: 4096,
            history_capacity: 512,
        }
    }
}

/// Run until SIGINT or SIGTERM, or until a task fails.
pub async fn run(layout: Layout, settings: Settings) -> Result<(), BoxError> {
    let listener = bind(&layout.socket).await?;
    let layout = Arc::new(layout);

    let (inbox_tx, inbox_rx) = mpsc::channel(settings.inbox_capacity);
    let (view_tx, view_rx) = watch::channel(Arc::new(View::render(&Board::default())));
    let (effect_tx, effect_rx) = mpsc::unbounded_channel();
    let (history_tx, history_rx) = mpsc::channel(settings.history_capacity);

    let mut tasks = JoinSet::new();
    tasks.spawn(core::run(Board::default(), inbox_rx, view_tx, effect_tx));
    tasks.spawn(rpc::serve(
        listener,
        inbox_tx.clone(),
        view_rx.clone(),
        layout.proc_root.clone(),
    ));
    tasks.spawn(scanner::run(
        layout.clone(),
        inbox_tx.clone(),
        settings.scan_every,
    ));
    tasks.spawn(effects::run(
        effect_rx,
        inbox_tx,
        history_tx,
        layout.clone(),
        settings.clone(),
    ));
    tasks.spawn(history::run(history_rx, layout.history_dir.clone()));
    tasks.spawn(publish::run(
        view_rx,
        layout.state_file.clone(),
        settings.coalesce,
    ));

    let mut term = signal(SignalKind::terminate())?;
    let outcome = tokio::select! {
        _ = tokio::signal::ctrl_c() => Ok(()),
        _ = term.recv() => Ok(()),
        finished = tasks.join_next() => match finished {
            Some(Ok(())) => Err("a daemon task exited unexpectedly".into()),
            Some(Err(e)) => Err(Box::new(e) as BoxError),
            None => Ok(()),
        },
    };

    tasks.shutdown().await;
    let _ = std::fs::remove_file(&layout.socket);
    outcome
}

/// Bind the socket, refusing to steal it from a live daemon and clearing one
/// left behind by a dead one. Mode 0600: hooks carry session metadata.
pub async fn bind(path: &PathBuf) -> Result<UnixListener, BoxError> {
    use std::os::unix::fs::PermissionsExt;

    if tokio::net::UnixStream::connect(path).await.is_ok() {
        return Err(format!("a daemon is already listening on {}", path.display()).into());
    }
    match std::fs::remove_file(path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(listener)
}
