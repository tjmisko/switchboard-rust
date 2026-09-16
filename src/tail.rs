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
        let mut polled = Polled::default();

        let len = file.seek(SeekFrom::End(0))?;
        if len < self.offset {
            // Shrunk under us: whatever we were carrying belongs to a file
            // that no longer exists.
            self.offset = 0;
            self.carry.clear();
            self.skipping = false;
            polled.truncated = true;
        }
        file.seek(SeekFrom::Start(self.offset))?;

        // Bound the read to what existed when we measured, so a writer that
        // keeps appending cannot keep us here forever.
        let mut reader = BufReader::new(file.take(len - self.offset));
        let mut chunk = Vec::new();
        loop {
            chunk.clear();
            let read = reader.read_until(b'\n', &mut chunk)?;
            if read == 0 {
                break;
            }
            self.offset += read as u64;

            let complete = chunk.last() == Some(&b'\n');
            if !complete {
                self.hold(&chunk, &mut polled);
                continue;
            }
            chunk.pop();
            if chunk.last() == Some(&b'\r') {
                chunk.pop();
            }
            self.finish(&chunk, &mut polled);
        }
        Ok(polled)
    }

    /// A fragment with no newline yet: carry it, unless the line is already
    /// too long to ever be wanted.
    fn hold(&mut self, fragment: &[u8], polled: &mut Polled) {
        if self.skipping {
            return;
        }
        if self.carry.len() + fragment.len() > self.max_line {
            self.skipping = true;
            self.carry.clear();
            polled.skipped += 1;
            return;
        }
        self.carry.extend_from_slice(fragment);
    }

    /// The end of a line: join it to the carry and decide what it is.
    fn finish(&mut self, rest: &[u8], polled: &mut Polled) {
        if self.skipping {
            // The oversized line was counted when the skip began.
            self.skipping = false;
            return;
        }
        let whole: Vec<u8> = if self.carry.is_empty() {
            rest.to_vec()
        } else {
            let mut joined = std::mem::take(&mut self.carry);
            joined.extend_from_slice(rest);
            joined
        };
        if whole.iter().all(u8::is_ascii_whitespace) {
            return;
        }
        if whole.len() > self.max_line {
            polled.skipped += 1;
            return;
        }
        match String::from_utf8(whole) {
            Ok(line) => polled.lines.push(line),
            Err(_) => polled.skipped += 1,
        }
    }
}

/// The offset of the first complete line within the last `window` bytes.
///
/// Used to replay only the recent end of a long transcript on discovery. When
/// the window starts mid-line, the partial line is skipped; when it covers the
/// whole file, the answer is zero.
pub fn tail_start<R: Read + Seek>(file: &mut R, window: u64) -> io::Result<u64> {
    let len = file.seek(SeekFrom::End(0))?;
    if window >= len {
        return Ok(0);
    }
    let start = len - window;

    // Is the byte just before the window a newline? Then the window begins
    // on a line boundary and the first line is whole.
    file.seek(SeekFrom::Start(start - 1))?;
    let mut before = [0u8; 1];
    file.read_exact(&mut before)?;
    if before[0] == b'\n' {
        return Ok(start);
    }

    // Mid-line: the first whole line begins after the next newline, or
    // nowhere in this file yet.
    let mut reader = BufReader::new(file.take(window));
    let mut rest = Vec::new();
    let read = reader.read_until(b'\n', &mut rest)?;
    match rest.last() {
        Some(b'\n') => Ok(start + read as u64),
        _ => Ok(len),
    }
}

/// Every non-blank line of a finite reader, with its 1-based line number.
/// Invalid UTF-8 surfaces as an `io::Error` of kind `InvalidData`.
pub fn numbered_lines<R: BufRead>(reader: R) -> impl Iterator<Item = (usize, io::Result<String>)> {
    reader
        .lines()
        .enumerate()
        .map(|(index, line)| (index + 1, line))
        .filter(|(_, line)| !line.as_ref().is_ok_and(|l| l.trim().is_empty()))
}
