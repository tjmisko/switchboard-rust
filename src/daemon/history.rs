//! The history appender task.

use std::fs::OpenOptions;
use std::path::PathBuf;
use std::time::SystemTime;

use tokio::sync::mpsc;

use crate::history::{HistoryWriter, Record, day_path};
use crate::model::HistoryEntry;

pub async fn run(mut entries: mpsc::Receiver<HistoryEntry>, dir: PathBuf) {
    while let Some(entry) = entries.recv().await {
        let record = Record::from_entry(&entry, SystemTime::now());
        let dir = dir.clone();
        let written = tokio::task::spawn_blocking(move || {
            std::fs::create_dir_all(&dir)?;
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(day_path(&dir, record.at))?;
            HistoryWriter::new(file).append(&record)
        })
        .await;
        match written {
            Ok(Ok(())) => {}
            Ok(Err(e)) => eprintln!("switchboard: history append failed: {e}"),
            Err(e) => eprintln!("switchboard: history task failed: {e}"),
        }
    }
}
