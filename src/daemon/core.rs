//! The core task: sole owner of the board.
//!
//! It waits on exactly two things — the next input, and the next deadline the
//! board says it has — and after either it applies the reducer, publishes the
//! view if it changed, and hands every other effect to the runner. It never
//! does I/O and never awaits anything that can stall.

use std::future;
use std::sync::Arc;

use tokio::sync::{mpsc, watch};
use tokio::time::{Instant as TokioInstant, sleep_until};

use crate::daemon::now;
use crate::ingest::Input;
use crate::model::{Board, Effect};
use crate::reduce::{self, next_deadline};
use crate::view::View;

/// Run until the inbox closes (every sender dropped). Returns the board, which
/// is what lets a test inspect it after driving the loop.
pub async fn run_until_closed(
    mut board: Board,
    mut inbox: mpsc::Receiver<Input>,
    view: watch::Sender<Arc<View>>,
    effects: mpsc::UnboundedSender<Effect>,
) -> Board {
    loop {
        let deadline = next_deadline(&board);
        let wake = async move {
            match deadline {
                Some(at) => sleep_until(TokioInstant::from_std(at)).await,
                None => future::pending::<()>().await,
            }
        };
        let produced = tokio::select! {
            input = inbox.recv() => match input {
                Some(input) => reduce::apply(&mut board, input),
                None => break,
            },
            () = wake => reduce::tick(&mut board, now()),
        };
        dispatch(&board, produced, &view, &effects);
    }
    board
}

/// The task body the daemon spawns.
pub async fn run(
    board: Board,
    inbox: mpsc::Receiver<Input>,
    view: watch::Sender<Arc<View>>,
    effects: mpsc::UnboundedSender<Effect>,
) {
    run_until_closed(board, inbox, view, effects).await;
}

fn dispatch(
    board: &Board,
    produced: Vec<Effect>,
    view: &watch::Sender<Arc<View>>,
    effects: &mpsc::UnboundedSender<Effect>,
) {
    for effect in produced {
        if effect == Effect::PublishState {
            publish(board, view);
            continue;
        }
        // A closed runner means the daemon is shutting down; nothing to do.
        let _ = effects.send(effect);
    }
}

/// Replace the published view only if it differs, so subscribers wake only
/// for real changes.
fn publish(board: &Board, view: &watch::Sender<Arc<View>>) {
    let rendered = View::render(board);
    view.send_if_modified(|current| {
        if **current == rendered {
            return false;
        }
        *current = Arc::new(rendered);
        true
    });
}
