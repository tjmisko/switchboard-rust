//! Following a file that another process appends to, one line at a time.
//!
//! Three things make this harder than `BufRead::lines`:
//!
//!   * The writer is mid-line whenever we look. A trailing fragment with no
//!     `\n` is not a line yet; it is carried until the rest arrives.
//!   * The file can shrink (rewritten, truncated, replaced). An offset past
//!     the end means start again from zero, not wait forever.
//!   * A single line can be enormous, or not UTF-8. Neither may stall the
//!     tail: oversized lines are skipped whole, invalid ones dropped.
//!
//! Generic over `Read + Seek`, so tests drive it with `Cursor<Vec<u8>>` and
//! the daemon with `File`.

use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};

/// Lines longer than this are skipped. A transcript line with a large tool
/// result can reach hundreds of KiB; nothing the adapters read is that big.
pub const DEFAULT_MAX_LINE: usize = 1 << 20;

/// A cursor into an append-only file.
#[derive(Debug, Clone)]
pub struct Tail {
    /// Bytes of the file consumed so far, including `carry`.
    offset: u64,
    /// The start of a line whose `\n` has not been written yet.
    carry: Vec<u8>,
    /// Set while discarding the rest of an oversized line.
    skipping: bool,
    max_line: usize,
}

impl Default for Tail {
    fn default() -> Self {
        Tail::new()
    }
}

/// What one `poll` found.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Polled {
    pub lines: Vec<String>,
    /// The file was shorter than our offset, so reading restarted at zero.
    pub truncated: bool,
    /// Lines dropped for being oversized or not UTF-8.
    pub skipped: usize,
}

impl Tail {
    pub fn new() -> Tail {
        Tail::starting_at(0)
    }

    /// Resume from a byte offset, which must be the start of a line.
    pub fn starting_at(offset: u64) -> Tail {
        Tail {
            offset,
            carry: Vec::new(),
            skipping: false,
            max_line: DEFAULT_MAX_LINE,
        }
    }

    pub fn with_max_line(mut self, max_line: usize) -> Tail {
        self.max_line = max_line;
        self
    }

    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Read whatever has been appended since the last poll.
    pub fn poll<R: Read + Seek>(&mut self, file: &mut R) -> io::Result<Polled> {
        todo!("T04")
    }
}

/// The offset of the first complete line within the last `window` bytes.
///
/// Used to replay only the recent end of a long transcript on discovery. When
/// the window starts mid-line, the partial line is skipped; when it covers the
/// whole file, the answer is zero.
pub fn tail_start<R: Read + Seek>(file: &mut R, window: u64) -> io::Result<u64> {
    todo!("T04")
}

/// Every non-blank line of a finite reader, with its 1-based line number.
/// Invalid UTF-8 surfaces as an `io::Error` of kind `InvalidData`.
pub fn numbered_lines<R: BufRead>(reader: R) -> impl Iterator<Item = (usize, io::Result<String>)> {
    todo!("T04");
    #[allow(unreachable_code)]
    std::iter::empty()
}
