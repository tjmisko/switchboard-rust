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
        let mut v = json!({
            "ts": format_rfc3339(self.at),
            "type": self.type_name(),
            "pid": self.pid,
        });
        if let Some(s) = &self.session {
            v["session_id"] = s.as_str().into();
        }
        match &self.kind {
            RecordKind::SessionStart { cwd } => {
                v["cwd"] = cwd.to_string_lossy().into();
            }
            RecordKind::SessionEnd => {}
            RecordKind::Shown { from, to, held } => {
                v["from"] = from.as_str().into();
                v["to"] = to.as_str().into();
                v["held_ms"] = (held.as_millis() as u64).into();
            }
        }
        v
    }

    /// `Ok(None)` for a line of a type this crate does not write.
    pub fn from_json(v: &Value) -> Result<Option<Record>, RecordError> {
        let kind_name = str_at(v, "type").ok_or(RecordError::Missing("type"))?;

        let kind = match kind_name {
            "session_start" => {
                let cwd = str_at(v, "cwd").ok_or(RecordError::Missing("cwd"))?;
                RecordKind::SessionStart {
                    cwd: PathBuf::from(cwd),
                }
            }
            "session_end" => RecordKind::SessionEnd,
            "shown" => RecordKind::Shown {
                from: phase_at(v, "from")?,
                to: phase_at(v, "to")?,
                held: Duration::from_millis(u64_at(v, "held_ms").unwrap_or(0)),
            },
            _ => return Ok(None),
        };

        let ts = str_at(v, "ts").ok_or(RecordError::Missing("ts"))?;
        let at = parse_rfc3339(ts).ok_or_else(|| RecordError::BadValue {
            field: "ts",
            value: ts.into(),
        })?;
        let pid = u64_at(v, "pid")
            .ok_or(RecordError::Missing("pid"))
            .and_then(|n| {
                u32::try_from(n).map_err(|_| RecordError::BadValue {
                    field: "pid",
                    value: n.to_string().into(),
                })
            })?;
        let session = str_at(v, "session_id").map(SessionId::from);

        Ok(Some(Record {
            at,
            pid,
            session,
            kind,
        }))
    }
}

fn phase_at(v: &Value, field: &'static str) -> Result<Phase, RecordError> {
    let name = str_at(v, field).ok_or(RecordError::Missing(field))?;
    Ok(name.parse::<Phase>()?)
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
        let mut line = record.to_json().to_string();
        line.push('\n');
        self.out.write_all(line.as_bytes())
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
    let mut day = DayFile::default();
    for (number, line) in numbered_lines(reader) {
        let parsed = line
            .map_err(RecordError::from)
            .and_then(|l| serde_json::from_str::<Value>(&l).map_err(RecordError::from))
            .and_then(|v| Record::from_json(&v));
        match parsed {
            Ok(Some(record)) => day.records.push(record),
            Ok(None) => day.foreign += 1,
            Err(RecordError::Io(e)) if e.kind() != io::ErrorKind::InvalidData => {
                day.errors.push((number, RecordError::Io(e)));
                break;
            }
            Err(e) => day.errors.push((number, e)),
        }
    }
    day
}

/// `<dir>/<YYYY-MM-DD>.jsonl` for a UTC day.
pub fn day_path(dir: &std::path::Path, at: SystemTime) -> PathBuf {
    let stamp = format_rfc3339(at);
    dir.join(format!("{}.jsonl", &stamp[..10]))
}
