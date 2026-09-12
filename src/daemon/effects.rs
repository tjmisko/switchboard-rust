//! The effect runner: turns the reducer's requests into tasks and I/O.
//!
//! It owns the transcript watchers, keyed by process, so replacing one on
//! `/clear` and ending one on death are a map insert and a map remove.

use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::daemon::Settings;
use crate::daemon::watcher::{self, WatchSpec};
use crate::ingest::Input;
use crate::model::{Effect, HistoryEntry, ProcKey};
use crate::paths::{Layout, transcript_path};
use crate::reduce::default_account;

pub async fn run(
    mut effects: mpsc::UnboundedReceiver<Effect>,
    inbox: mpsc::Sender<Input>,
    history: mpsc::Sender<HistoryEntry>,
    layout: Arc<Layout>,
    settings: Settings,
) {
    todo!("T12")
}

/// Watcher tasks by process. Aborts every task it still holds when dropped,
/// so a runner that exits cannot leak watchers.
#[derive(Default)]
pub struct Watchers {
    tasks: HashMap<ProcKey, JoinHandle<()>>,
}

impl Watchers {
    /// Start a watcher, aborting any previous one for the same process.
    pub fn start(&mut self, process: ProcKey, task: JoinHandle<()>) {
        todo!("T12")
    }

    pub fn stop(&mut self, process: &ProcKey) {
        todo!("T12")
    }

    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }
}

impl Drop for Watchers {
    fn drop(&mut self) {
        // T12: abort every task still held.
        //
        // Not a `todo!()`: a panic inside `drop` while a failing test is
        // already unwinding aborts the whole test binary.
    }
}
