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
use crate::ingest::json::{str_at, u64_at};
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
/// field 3 (`state`).
pub fn parse_stat(s: &str) -> Result<Stat, StatError> {
    let open = s.find('(').ok_or(StatError::NoComm)?;
    let close = s.rfind(')').ok_or(StatError::NoComm)?;
    if close < open {
        return Err(StatError::NoComm);
    }

    let pid = number(s[..open].trim(), "pid")?;
    let comm = s[open + 1..close].to_owned();

    let fields: Vec<&str> = s[close + 1..].split_whitespace().collect();
    // `fields[0]` is stat field 3, so field N is `fields[N - 3]`.
    let field =
        |n: usize, name: &'static str| fields.get(n - 3).copied().ok_or(StatError::Missing(name));

    let state = field(3, "state")?
        .chars()
        .next()
        .ok_or(StatError::Missing("state"))?;
    let ppid = number(field(4, "ppid")?, "ppid")?;
    let tty_nr = number(field(7, "tty_nr")?, "tty_nr")?;
    let utime: u64 = number(field(14, "utime")?, "utime")?;
    let stime: u64 = number(field(15, "stime")?, "stime")?;
    let start = number(field(22, "starttime")?, "starttime")?;

    Ok(Stat {
        pid,
        comm,
        state,
        ppid,
        tty_nr,
        cpu_ticks: utime + stime,
        start,
    })
}

fn number<T: std::str::FromStr<Err = std::num::ParseIntError>>(
    text: &str,
    field: &'static str,
) -> Result<T, StatError> {
    text.parse()
        .map_err(|source| StatError::BadNumber { field, source })
}

/// `/proc/<pid>/cmdline`: NUL-separated, NUL-terminated, not necessarily
/// UTF-8. Each argument borrows from `bytes` unless it had to be repaired.
pub fn parse_cmdline(bytes: &[u8]) -> Vec<Cow<'_, str>> {
    bytes
        .split(|&b| b == 0)
        .filter(|arg| !arg.is_empty())
        .map(String::from_utf8_lossy)
        .collect()
}

/// Whether a process is an interactive Claude session.
///
/// `exe` is `None` when the link cannot be read, which for another user's
/// process is normal; the check then rests on `comm` and `argv` alone.
pub fn classify<S: AsRef<str>>(comm: &str, exe: Option<&Path>, argv: &[S]) -> Option<Provider> {
    if comm != "claude" {
        return None;
    }
    if let Some(exe) = exe
        && !exe.to_string_lossy().contains("/claude/")
    {
        return None;
    }
    let mut args = argv.iter().map(AsRef::as_ref);
    let _program = args.next();
    if matches!(args.clone().next(), Some("daemon" | "mcp")) {
        return None;
    }
    if args.any(|a| a == "-p" || a == "--print") {
        return None;
    }
    Some(Provider::Claude)
}

/// A session file `~/.claude/sessions/<pid>.json` names the conversation only
/// if it was written by this very process: `procStart` is a string and must
/// equal the stat start time.
pub fn session_from_file(v: &Value, key: ProcKey) -> Option<SessionId> {
    let pid_matches = u64_at(v, "pid") == Some(u64::from(key.pid));
    let start_matches = str_at(v, "procStart")
        .and_then(|s| s.parse::<u64>().ok())
        .is_some_and(|start| start == key.start);
    if !(pid_matches && start_matches) {
        return None;
    }
    str_at(v, "sessionId")
        .filter(|s| !s.is_empty())
        .map(SessionId::from)
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

    let session = fs::read_to_string(sessions.join(format!("{pid}.json")))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|v| session_from_file(&v, key));

    Some(ProcessRow {
        key,
        provider,
        cwd,
        session,
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
    let text = fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    parse_stat(&text)
        .ok()
        .map(|s| s.ppid)
        .filter(|&ppid| ppid != 0)
}

/// `pid`, its parent, its grandparent, … at most `limit` of them.
///
/// A hook runs as `claude -> sh -> switchboard-hook`, so the pid the
/// forwarder reports is rarely the agent itself.
pub fn ancestors(proc_root: &Path, pid: u32, limit: usize) -> impl Iterator<Item = u32> + '_ {
    std::iter::successors(Some(pid), move |&p| parent_of(proc_root, p)).take(limit)
}

/// The start time of `pid`, to turn a bare pid into a `ProcKey`.
pub fn key_of(proc_root: &Path, pid: u32) -> Option<ProcKey> {
    let text = fs::read_to_string(proc_root.join(pid.to_string()).join("stat")).ok()?;
    parse_stat(&text).ok().map(|s| s.key())
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
