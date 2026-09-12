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
    let mut watchers = Watchers::default();

    while let Some(effect) = effects.recv().await {
        match effect {
            Effect::WatchTranscript {
                process,
                session,
                cwd,
            } => {
                let spec = WatchSpec {
                    path: transcript_path(&layout.claude_projects, &cwd, &session),
                    process,
                    session,
                    account: default_account(),
                    poll: settings.transcript_poll,
                    replay_bytes: settings.replay_bytes,
                };
                watchers.start(process, tokio::spawn(watcher::run(spec, inbox.clone())));
            }
            Effect::Forget { process } => watchers.stop(&process),
            Effect::AppendHistory(entry) => {
                // History is best-effort. A full appender drops rather than
                // letting a slow disk back up into the core.
                if history.try_send(entry).is_err() {
                    eprintln!("switchboard: history appender full, entry dropped");
                }
            }
            // The core publishes; it never forwards this.
            Effect::PublishState => {}
        }
    }
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
        if let Some(previous) = self.tasks.insert(process, task) {
            previous.abort();
        }
    }

    pub fn stop(&mut self, process: &ProcKey) {
        if let Some(task) = self.tasks.remove(process) {
            task.abort();
        }
    }

    /// Abort every task still held.
    pub fn abort_all(&mut self) {
        for (_, task) in self.tasks.drain() {
            task.abort();
        }
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
        self.abort_all();
    }
}
