//! Writing `state.json` when the view changes.

use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use tokio::sync::watch;

use crate::daemon::now;
use crate::paths::temp_sibling;
use crate::view::View;

/// Wait for a change, let further changes pile up for `coalesce`, then write
/// the latest view once. A burst of transitions inside the calm band costs
/// one write, not one per transition (R6).
pub async fn run(mut view: watch::Receiver<Arc<View>>, path: PathBuf, coalesce: Duration) {
    loop {
        if view.changed().await.is_err() {
            return;
        }
        tokio::time::sleep(coalesce).await;
        let latest = Arc::clone(&view.borrow_and_update());
        let body = latest.to_json(now(), SystemTime::now()).to_string();
        if let Err(e) = write_atomic(&path, body.as_bytes()).await {
            eprintln!("switchboard: cannot write {}: {e}", path.display());
        }
    }
}

/// Write to a sibling temporary file, then rename over the target. A reader
/// sees the old file or the new one, never half of either.
pub async fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let tmp = temp_sibling(path)
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    tokio::fs::write(&tmp, bytes).await?;
    tokio::fs::rename(&tmp, path).await
}
