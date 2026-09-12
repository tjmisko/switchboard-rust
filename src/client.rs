//! Talking to the daemon from a short-lived process, synchronously.
//!
//! The hook forwarder runs inside the agent's hook budget, so it uses plain
//! `std` sockets with timeouts: no runtime to start, nothing to spawn.

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;

use serde_json::Value;

use crate::error::ForwardError;
use crate::ingest::forward::interpret_reply;

/// Send one frame and read one verdict.
///
/// Each step maps its `io::Error` to the variant that says which step failed,
/// which is why there is no blanket `From<io::Error>` for `ForwardError`.
pub fn send(socket: &Path, frame: &Value, timeout: Duration) -> Result<(), ForwardError> {
    todo!("T02")
}
