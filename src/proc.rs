//! Finding agent processes in `/proc`.
//!
//! The parsers are pure functions of file contents. `sweep` and `ancestors`
//! do the reading, against a root the caller passes in, so tests can build a
//! fake `/proc` in a temporary directory.

use std::borrow::Cow;
use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::error::StatError;
use crate::ingest::json::{nonblank_str_at, str_at, u64_at};
use crate::model::{ProcKey, ProcessRow, Provider, SessionId};

/// The fields of `/proc/<pid>/stat` the daemon uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stat {
    pub pid: u32,
    /// Truncated by the kernel to 15 bytes.
    pub comm: String,
    /// `R`, `S`, `T` (stopped), `Z` (zombie), …
    pub state: char,
    pub ppid: u32,
    pub tty_nr: u64,
    /// `utime + stime`, in clock ticks.
    pub cpu_ticks: u64,
    /// Field 22: clock ticks after boot. With `pid`, a process identity that
    /// survives pid reuse.
    pub start: u64,
}

impl Stat {
    pub fn key(&self) -> ProcKey {
        ProcKey {
            pid: self.pid,
            start: self.start,
        }
    }
}

/// Parse one `stat` line.
///
/// `comm` is wrapped in parentheses and may itself contain spaces and `)`, so
/// the only safe split is at the *last* `)`. Fields after it are counted from
/// field 3 (`state`), so field N is `fields[N - 3]` once the text after the
/// paren is split on whitespace. The fields this reads, numbered as
/// `proc_pid_stat(5)` numbers them:
///
/// | field | name        | goes to          |
/// |-------|-------------|------------------|
/// | 1     | `pid`       | `pid` (before the paren) |
/// | 2     | `comm`      | `comm` (inside the parens) |
/// | 3     | `state`     | `state`, its first character |
/// | 4     | `ppid`      | `ppid`           |
/// | 7     | `tty_nr`    | `tty_nr`         |
/// | 14    | `utime`     | `cpu_ticks`, summed with |
/// | 15    | `stime`     | `cpu_ticks`      |
/// | 22    | `starttime` | `start`          |
///
/// A missing field is `Missing(name)` with that name; a field that will not
/// parse is `BadNumber { field: name, .. }`. No paren pair, or a `)` before
/// the `(`, is `NoComm`.
pub fn parse_stat(s: &str) -> Result<Stat, StatError> {
    todo!("T05")
}

fn number<T: std::str::FromStr<Err = std::num::ParseIntError>>(
    text: &str,
    field: &'static str,
) -> Result<T, StatError> {
    todo!("T05")
}

/// `/proc/<pid>/cmdline`: NUL-separated, NUL-terminated, not necessarily
/// UTF-8. Each argument borrows from `bytes` unless it had to be repaired.
pub fn parse_cmdline(bytes: &[u8]) -> Vec<Cow<'_, str>> {
    todo!("T05")
}

/// Whether a process is an interactive Claude session.
///
/// The rules, in the order they are checked:
///
/// 1. `comm` must be exactly `claude`. Anything else is not Claude, whatever
///    its arguments say.
/// 2. A readable `exe` must have `/claude/` somewhere in its path (the
///    harness installs under `.../share/claude/versions/<v>`). `exe` is
///    `None` when the link cannot be read, which for another user's process
///    is normal; the check then rests on `comm` and `argv` alone, so `None`
///    is not a rejection.
/// 3. A service is not a session: `argv[1]` (the verb, right after the
///    program) equal to `daemon` or `mcp` is rejected. Only the verb counts,
///    so `claude --model daemon` is still a session.
/// 4. Headless is not a session: `-p` or `--print` anywhere after the
///    program is rejected.
///
/// Everything else is `Provider::Claude`.
pub fn classify<S: AsRef<str>>(comm: &str, exe: Option<&Path>, argv: &[S]) -> Option<Provider> {
    todo!("T05")
}

/// A session file `~/.claude/sessions/<pid>.json` names the conversation only
/// if it was written by this very process: `procStart` is a string and must
/// equal the stat start time.
pub fn session_from_file(v: &Value, key: ProcKey) -> Option<SessionId> {
    todo!("T05")
}

/// The same file's `name`: the display name the operator sees in Claude's
/// own UI (`switchboard-rust-7f`), derived by the harness or set with
/// `/rename`. Under the same guard as the session, for the same reason.
pub fn name_from_file(v: &Value, key: ProcKey) -> Option<Box<str>> {
    todo!("T05")
}

/// Whether a session file was written by the process `key` names.
fn written_by(v: &Value, key: ProcKey) -> bool {
    todo!("T05")
}

/// Read and classify one `/proc/<pid>`. `None` for anything that is not a
/// live Claude session, or that vanished while being read.
pub fn read_process(proc_root: &Path, sessions: &Path, pid: u32) -> Option<ProcessRow> {
    let dir = proc_root.join(pid.to_string());
    let stat = parse_stat(&fs::read_to_string(dir.join("stat")).ok()?).ok()?;
    if stat.state == 'Z' {
        return None;
    }
    let exe = fs::read_link(dir.join("exe")).ok();
    let cmdline = fs::read(dir.join("cmdline")).ok()?;
    let argv = parse_cmdline(&cmdline);
    let provider = classify(&stat.comm, exe.as_deref(), &argv)?;
    let cwd = fs::read_link(dir.join("cwd")).ok()?;
    let key = stat.key();

    let session_file = fs::read_to_string(sessions.join(format!("{pid}.json")))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let session = session_file
        .as_ref()
        .and_then(|v| session_from_file(v, key));
    let name = session_file.as_ref().and_then(|v| name_from_file(v, key));

    Some(ProcessRow {
        key,
        provider,
        cwd,
        session,
        name,
        tty: (stat.tty_nr != 0).then_some(stat.tty_nr),
        cpu_ticks: stat.cpu_ticks,
    })
}

/// Every Claude session process under `proc_root`, in pid order.
pub fn sweep(proc_root: &Path, sessions: &Path) -> Vec<ProcessRow> {
    let Ok(entries) = fs::read_dir(proc_root) else {
        return Vec::new();
    };
    let mut pids: Vec<u32> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_str()?.parse().ok())
        .collect();
    pids.sort_unstable();
    pids.into_iter()
        .filter_map(|pid| read_process(proc_root, sessions, pid))
        .collect()
}

/// The parent of `pid`, or `None` at the root or when it cannot be read.
pub fn parent_of(proc_root: &Path, pid: u32) -> Option<u32> {
    todo!("T05")
}

/// `pid`, its parent, its grandparent, … at most `limit` of them.
///
/// The chain starts with `pid` itself whether or not it can be read, and
/// stops at the first parent that cannot be, or at `limit`. A `limit` of
/// zero yields nothing, not even `pid`.
///
/// A hook runs as `claude -> sh -> switchboard-hook`, so the pid the
/// forwarder reports is rarely the agent itself.
pub fn ancestors(proc_root: &Path, pid: u32, limit: usize) -> impl Iterator<Item = u32> + '_ {
    // T05. Replace this whole body. The `empty()` below only keeps the
    // `impl Iterator` signature compiling until you do; it is not a start.
    todo!("T05");
    #[allow(unreachable_code)]
    std::iter::empty()
}

/// The start time of `pid`, to turn a bare pid into a `ProcKey`. Reads the
/// same `stat` file as `parent_of`; the two can share a helper.
pub fn key_of(proc_root: &Path, pid: u32) -> Option<ProcKey> {
    todo!("T05")
}

#[cfg(test)]
mod tests {
    use super::*;

    const REAL: &str = "6002 (claude) S 5890 6002 5890 34818 6002 4194560 1311934 0 1085 0 43219 6390 0 0 20 0 19 0 12313 75436916736 125906 18446744073709551615 1 1 0 0 0 0 0 16781312 1024 0 0 0 17 5 0 0 0 0 0";

    #[test]
    fn should_read_start_time_from_field_twenty_two() {
        let s = parse_stat(REAL).unwrap();
        assert_eq!(
            s.key(),
            ProcKey {
                pid: 6002,
                start: 12313
            }
        );
        assert_eq!(s.ppid, 5890);
        assert_eq!(s.cpu_ticks, 43219 + 6390);
    }

    #[test]
    fn should_split_at_the_last_paren_when_comm_contains_one() {
        let tricky = REAL.replace("(claude)", "(a) b (c)");
        let s = parse_stat(&tricky).unwrap();
        assert_eq!(s.comm, "a) b (c");
        assert_eq!(s.start, 12313);
    }
}
