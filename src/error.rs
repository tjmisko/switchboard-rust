//! Error types, one per boundary, written by hand.
//!
//! The rule that shapes all of them: **absence is not failure.** An optional
//! field that is missing is `Option::None` and travels as data. It becomes an
//! error only at the point where the caller required it, via `ok_or` /
//! `ok_or_else`, and the error then names the field. A field that is present
//! but malformed is always an error, which is why so many parsers here end in
//! `Option<Result<..>>::transpose()`.
//!
//! Each enum separates failures by what the caller can do about them:
//!
//!   transport   the bytes never arrived          (retry, or give up quietly)
//!   syntax      the bytes are not JSON / a number (the sender is broken)
//!   shape       valid JSON, wrong structure       (a version mismatch)
//!   refusal     well-formed, and the peer said no (report it)
//!
//! `Display` and `source` overlap here on purpose. A variant that wraps
//! another error both names it in its `Display` (`malformed JSON: expected
//! value at line 1 column 1`) and returns it from `source`. The one-line form
//! is what the two consumers print: `main` and the hook forwarder each write
//! a single `eprintln!("…: {e}")` and never walk the chain. `source` is kept
//! for a reporter that does walk it, which will then print the cause twice;
//! that is the accepted price. Some guides argue for the opposite split
//! (`Display` says only what this layer added, `source` carries the rest).
//! Not here: every enum in this file follows the rule above, and the tests
//! pin the `malformed JSON: ` prefix together with a present `source`.

use std::error::Error;
use std::fmt;
use std::io;
use std::num::ParseIntError;

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

/// A string that is not in a closed vocabulary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownName {
    pub vocabulary: &'static str,
    pub value: Box<str>,
}

impl UnknownName {
    pub fn new(vocabulary: &'static str, value: &str) -> UnknownName {
        UnknownName {
            vocabulary,
            value: value.into(),
        }
    }
}

impl fmt::Display for UnknownName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("T02")
    }
}

impl Error for UnknownName {}

// ---------------------------------------------------------------------------
// Socket frames
// ---------------------------------------------------------------------------

/// A request line on the daemon socket that cannot be understood.
///
/// Note what is *not* here: an unsupported hook event. That is a well-formed
/// frame about something the model does not track, and it decodes to
/// `Ok(None)`.
#[derive(Debug)]
pub enum FrameError {
    /// Not JSON at all.
    Json(serde_json::Error),
    /// JSON, but not an object.
    NotAnObject,
    /// A required field is absent.
    Missing(&'static str),
    /// A field is present with the wrong JSON type or an out-of-range value.
    WrongType {
        field: &'static str,
        expected: &'static str,
    },
    /// `cmd` names nothing the daemon serves.
    UnknownCommand(Box<str>),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("T02")
    }
}

impl Error for FrameError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        todo!("T02")
    }
}

impl From<serde_json::Error> for FrameError {
    fn from(e: serde_json::Error) -> FrameError {
        todo!("T02")
    }
}

// ---------------------------------------------------------------------------
// /proc
// ---------------------------------------------------------------------------

/// `/proc/<pid>/stat` that does not parse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatError {
    /// No `(comm)` section: the parentheses are missing or out of order.
    NoComm,
    /// The line ended before this field.
    Missing(&'static str),
    /// The field is there and is not a number.
    BadNumber {
        field: &'static str,
        source: ParseIntError,
    },
}

impl fmt::Display for StatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("T02")
    }
}

impl Error for StatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        todo!("T02")
    }
}

// ---------------------------------------------------------------------------
// History
// ---------------------------------------------------------------------------

/// One history line that cannot become a `Record`.
#[derive(Debug)]
pub enum RecordError {
    Io(io::Error),
    Json(serde_json::Error),
    Missing(&'static str),
    BadValue {
        field: &'static str,
        value: Box<str>,
    },
    Name(UnknownName),
}

impl fmt::Display for RecordError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("T02")
    }
}

impl Error for RecordError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        todo!("T02")
    }
}

impl From<io::Error> for RecordError {
    fn from(e: io::Error) -> RecordError {
        todo!("T02")
    }
}

impl From<serde_json::Error> for RecordError {
    fn from(e: serde_json::Error) -> RecordError {
        todo!("T02")
    }
}

impl From<UnknownName> for RecordError {
    fn from(e: UnknownName) -> RecordError {
        todo!("T02")
    }
}

// ---------------------------------------------------------------------------
// Hook forwarder
// ---------------------------------------------------------------------------

/// Why a hook frame did not land. The forwarder never lets any of these reach
/// the agent — it exits 0 regardless — but it can say which one happened.
///
/// The variants are the four questions in order: did we reach the daemon, did
/// the bytes move, did it answer in a language we speak, and did it say yes.
#[derive(Debug)]
pub enum ForwardError {
    /// Transport: no daemon listening, or the socket is not ours.
    Connect(io::Error),
    /// Transport: connected, then a read or write failed or timed out.
    Io(io::Error),
    /// The reply is not JSON.
    Malformed(serde_json::Error),
    /// The reply is JSON without a boolean `ok`.
    NoVerdict,
    /// The daemon understood and refused.
    Rejected(Box<str>),
}

impl fmt::Display for ForwardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        todo!("T02")
    }
}

impl Error for ForwardError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        todo!("T02")
    }
}

/// Deliberately no `From<io::Error>`: a bare `?` could not tell `Connect`
/// from `Io`, and that difference is the point of the type.
impl From<serde_json::Error> for ForwardError {
    fn from(e: serde_json::Error) -> ForwardError {
        todo!("T02")
    }
}

/// The error type for code that only reports, never branches: `main`, and
/// tests. Anything that implements `Error` converts into it with `?`.
pub type BoxError = Box<dyn Error + Send + Sync + 'static>;
