//! The history day file: one JSON object per line.
//!
//! Writing is generic over `Write` and reading over `BufRead`, so the same
//! code serves a `File`, a `BufWriter<File>`, a `Vec<u8>` in a test, and
//! stdin in a debugging tool.
//!
//! The reader is tolerant in the same way the adapters are. A line of a type
//! this crate does not write — the Go daemon's `usage_sample`, `focus`, … —
//! is skipped silently. A line of a type it does write but cannot parse is an
//! error, reported with its line number, and reading continues.

use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use serde_json::{Value, json};

use crate::error::RecordError;
use crate::ingest::json::{str_at, u64_at};
use crate::model::{HistoryEntry, Phase, ProcKey, SessionId};
use crate::tail::numbered_lines;
use crate::time::{format_rfc3339, parse_rfc3339};

/// One history line, as this crate writes it.
#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub at: SystemTime,
    pub pid: u32,
    pub session: Option<SessionId>,
    pub kind: RecordKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum RecordKind {
    SessionStart {
        cwd: PathBuf,
    },
    SessionEnd,
    Shown {
        from: Phase,
        to: Phase,
        held: Duration,
    },
}

impl Record {
    /// Stamp a reducer `HistoryEntry` with the wall clock.
    pub fn from_entry(entry: &HistoryEntry, at: SystemTime) -> Record {
        let (process, session, kind): (&ProcKey, Option<&SessionId>, RecordKind) = match entry {
            HistoryEntry::AgentFound { process, cwd } => {
                (process, None, RecordKind::SessionStart { cwd: cwd.clone() })
            }
            HistoryEntry::AgentGone { process, session } => {
                (process, session.as_ref(), RecordKind::SessionEnd)
            }
            HistoryEntry::Shown {
                process,
                session,
                from,
                to,
                held,
            } => (
                process,
                session.as_ref(),
                RecordKind::Shown {
                    from: *from,
                    to: *to,
                    held: *held,
                },
            ),
        };
        Record {
            at,
            pid: process.pid,
            session: session.cloned(),
            kind,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self.kind {
            RecordKind::SessionStart { .. } => "session_start",
            RecordKind::SessionEnd => "session_end",
            RecordKind::Shown { .. } => "shown",
        }
    }

    pub fn to_json(&self) -> Value {
        todo!("T04")
    }

    /// `Ok(None)` for a line of a type this crate does not write.
    pub fn from_json(v: &Value) -> Result<Option<Record>, RecordError> {
        todo!("T04")
    }
}

fn phase_at(v: &Value, field: &'static str) -> Result<Phase, RecordError> {
    todo!("T04")
}

/// Appends records, one per line.
pub struct HistoryWriter<W: Write> {
    out: W,
}

impl<W: Write> HistoryWriter<W> {
    pub fn new(out: W) -> HistoryWriter<W> {
        HistoryWriter { out }
    }

    /// Write one record and its newline. The line is assembled first and
    /// written with a single `write_all`, so an appender sharing the file
    /// never interleaves inside a line.
    pub fn append(&mut self, record: &Record) -> io::Result<()> {
        todo!("T04")
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }

    pub fn get_ref(&self) -> &W {
        &self.out
    }

    pub fn into_inner(self) -> W {
        self.out
    }
}

/// Everything a day file holds, and everything wrong with it.
#[derive(Debug, Default)]
pub struct DayFile {
    pub records: Vec<Record>,
    /// `(line number, error)`, in file order.
    pub errors: Vec<(usize, RecordError)>,
    /// Well-formed lines of types this crate does not write.
    pub foreign: usize,
}

/// Read a whole day file. Never fails as a whole: a bad line is recorded
/// against its number and skipped. A read error other than invalid UTF-8 is
/// recorded the same way, and ends the read, because nothing after it can be
/// trusted to be where it claims.
pub fn read_day<R: BufRead>(reader: R) -> DayFile {
    todo!("T04")
}

/// `<dir>/<YYYY-MM-DD>.jsonl` for a UTC day.
pub fn day_path(dir: &std::path::Path, at: SystemTime) -> PathBuf {
    let stamp = format_rfc3339(at);
    dir.join(format!("{}.jsonl", &stamp[..10]))
}
