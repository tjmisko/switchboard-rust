//! Talking to the daemon from a short-lived process, synchronously.
//!
//! The hook forwarder runs inside the agent's hook budget, so it uses plain
//! `std` sockets with timeouts: no runtime to start, nothing to spawn.

use std::io::{self, BufRead, BufReader, Write};
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
    let mut stream = UnixStream::connect(socket).map_err(ForwardError::Connect)?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(ForwardError::Io)?;
    stream
        .set_write_timeout(Some(timeout))
        .map_err(ForwardError::Io)?;

    let mut line = frame.to_string();
    line.push('\n');
    stream
        .write_all(line.as_bytes())
        .map_err(ForwardError::Io)?;

    let mut reply = String::new();
    let read = BufReader::new(&stream)
        .read_line(&mut reply)
        .map_err(ForwardError::Io)?;
    if read == 0 {
        // The daemon hung up before answering: no reply exists to interpret.
        return Err(ForwardError::Io(io::Error::from(io::ErrorKind::UnexpectedEof)));
    }
    interpret_reply(reply.trim_end())
}
