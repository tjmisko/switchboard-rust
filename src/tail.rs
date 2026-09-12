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
            self.offset = 0;
            self.carry.clear();
            self.skipping = false;
            polled.truncated = true;
        }
        file.seek(SeekFrom::Start(self.offset))?;

        // `&mut R` is itself `Read`, so the reader borrows the file for the
        // loop and gives it back when it drops. `take` stops at the length we
        // measured, so a writer racing us cannot make one poll unbounded.
        let mut reader = BufReader::new(file.take(len - self.offset));
        let mut chunk = Vec::new();
        loop {
            chunk.clear();
            let n = reader.read_until(b'\n', &mut chunk)?;
            if n == 0 {
                break;
            }
            self.offset += n as u64;

            let complete = chunk.last() == Some(&b'\n');
            if self.skipping {
                if complete {
                    self.skipping = false;
                }
                continue;
            }
            self.carry.extend_from_slice(&chunk);
            if self.carry.len() > self.max_line {
                self.carry.clear();
                polled.skipped += 1;
                self.skipping = !complete;
                continue;
            }
            if !complete {
                break;
            }

            let mut bytes = std::mem::take(&mut self.carry);
            bytes.pop();
            if bytes.last() == Some(&b'\r') {
                bytes.pop();
            }
            match String::from_utf8(bytes) {
                Ok(line) if line.trim().is_empty() => {}
                Ok(line) => polled.lines.push(line),
                Err(_) => polled.skipped += 1,
            }
        }
        Ok(polled)
    }
}

/// The offset of the first complete line within the last `window` bytes.
///
/// Used to replay only the recent end of a long transcript on discovery. When
/// the window starts mid-line, the partial line is skipped; when it covers the
/// whole file, the answer is zero.
pub fn tail_start<R: Read + Seek>(file: &mut R, window: u64) -> io::Result<u64> {
    let len = file.seek(SeekFrom::End(0))?;
    if len <= window {
        return Ok(0);
    }
    let start = len - window;
    // Read from one byte early: if that byte is `\n`, `start` is already a
    // line boundary.
    file.seek(SeekFrom::Start(start - 1))?;
    let mut reader = BufReader::new(file.take(window + 1));
    let mut skipped = Vec::new();
    let n = reader.read_until(b'\n', &mut skipped)?;
    Ok(start - 1 + n as u64)
}

/// Every non-blank line of a finite reader, with its 1-based line number.
/// Invalid UTF-8 surfaces as an `io::Error` of kind `InvalidData`.
pub fn numbered_lines<R: BufRead>(reader: R) -> impl Iterator<Item = (usize, io::Result<String>)> {
    reader
        .lines()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .filter(|(_, line)| line.as_ref().map_or(true, |l| !l.trim().is_empty()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};

    #[test]
    fn should_carry_a_partial_line_until_its_newline_arrives() {
        let mut file = Cursor::new(b"one\ntw".to_vec());
        let mut tail = Tail::new();
        assert_eq!(tail.poll(&mut file).unwrap().lines, ["one"]);

        file.seek(SeekFrom::End(0)).unwrap();
        file.write_all(b"o\n").unwrap();
        assert_eq!(tail.poll(&mut file).unwrap().lines, ["two"]);
    }

    #[test]
    fn should_restart_from_zero_when_the_file_shrinks() {
        let mut tail = Tail::new();
        tail.poll(&mut Cursor::new(b"aaaa\nbbbb\n".to_vec()))
            .unwrap();
        let polled = tail.poll(&mut Cursor::new(b"c\n".to_vec())).unwrap();
        assert!(polled.truncated);
        assert_eq!(polled.lines, ["c"]);
    }

    #[test]
    fn should_find_the_first_line_boundary_inside_the_window() {
        let data = b"0123456789\nabc\ndef\n";
        let mut c = Cursor::new(data.to_vec());
        // window of 6 starts at 'c' in "abc", so skip to "def"
        let off = tail_start(&mut c, 6).unwrap();
        assert_eq!(&data[off as usize..], b"def\n");
        // window of 8 starts exactly at "abc"
        let off = tail_start(&mut c, 8).unwrap();
        assert_eq!(&data[off as usize..], b"abc\ndef\n");
    }
}
