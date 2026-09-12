//! `switchboard`: the daemon.

use std::process::ExitCode;

use switchboard::daemon::{self, Settings};
use switchboard::paths::Layout;

#[tokio::main]
async fn main() -> ExitCode {
    let Some(layout) = Layout::from_env() else {
        eprintln!("switchboard: $HOME is not set");
        return ExitCode::FAILURE;
    };
    match daemon::run(layout, Settings::default()).await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("switchboard: {e}");
            ExitCode::FAILURE
        }
    }
}
