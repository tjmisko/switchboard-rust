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
//! And one that follows from the first: a multibyte character can be split
//! across two polls. The carry is bytes, not text, and a line is decoded
//! only once its `\n` has arrived, so `caf` + `\xc3` in one poll and `\xa9`
//! in the next reassemble into `café`.
//!
//! Generic over `Read + Seek`, so tests drive it with `Cursor<Vec<u8>>` and
//! the daemon with `File`. `seek(SeekFrom::End(0))` returns the new
//! position, which is how both `poll` and `tail_start` learn the length.

use std::io::{self, BufRead, BufReader, Read, Seek, SeekFrom};

/// Lines longer than this are skipped. The measure is the line's bytes
/// including its terminator: with a limit of 8, `1234567\n` is kept and
/// `12345678\n` is not. A transcript line with a large tool result can reach
/// hundreds of KiB; nothing the adapters read is that big.
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
    /// Complete lines, without their terminators. Blank and whitespace-only
    /// lines are left out and not counted as skipped.
    pub lines: Vec<String>,
    /// The file was shorter than our offset, so reading restarted at zero.
    /// This also abandons any carried partial line and any skip in progress.
    pub truncated: bool,
    /// Lines dropped for being oversized or not UTF-8. An oversized line is
    /// counted in the poll where its bytes first exceed `max_line`, which
    /// can be before its `\n` arrives; the rest of it is then discarded
    /// silently, however many polls that takes.
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
/// Used to replay only the recent end of a long transcript on discovery.
/// When the window covers the whole file, the answer is zero. When it starts
/// exactly on a line boundary, the answer is that boundary. When it starts
/// mid-line, the partial line is skipped and the answer is the byte after
/// its `\n`. When the window holds no `\n` at all, nothing in it is a whole
/// line and the answer is the file's length.
pub fn tail_start<R: Read + Seek>(file: &mut R, window: u64) -> io::Result<u64> {
    todo!("T04")
}

/// Every non-blank line of a finite reader, with its 1-based line number.
/// Invalid UTF-8 surfaces as an `io::Error` of kind `InvalidData`, and the
/// iteration continues past it: `BufRead::lines` consumes the bad line's
/// bytes before reporting them, which is what lets `history::read_day` keep
/// going.
pub fn numbered_lines<R: BufRead>(reader: R) -> impl Iterator<Item = (usize, io::Result<String>)> {
    // The `empty()` below only keeps the `impl Iterator` signature compiling
    // while the body is missing. Replace the whole body, not just the `todo!`.
    todo!("T04");
    #[allow(unreachable_code)]
    std::iter::empty()
}
