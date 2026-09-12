# T04 — Line-oriented I/O: tail and history

**Core: 1h30** · Gaps: `Read` / `BufRead` / `Write`, generic `R: BufRead`,
why `BufReader` exists, `Cursor<Vec<u8>>` and `Vec<u8>` as test doubles,
`&mut R` as a reader · Tests: `cargo test --test t04_history`, and
**`t04_tail`, which you write**

The transcript is a file another process appends to. The history is a file
this process appends to. Both are one JSON object per line. Nothing in this
task knows about JSON or transcripts: `Tail` yields `String`s, `HistoryWriter`
takes `Record`s, and neither cares what the underlying reader or writer is.

## Implement

**`src/tail.rs`**:

- `Tail::poll<R: Read + Seek>(&mut self, file: &mut R) -> io::Result<Polled>`.
  Read the module comment's three hazards before you start: partial lines,
  shrinking files, and oversized or non-UTF-8 lines. Use `read_until(b'\n')`
  on a `BufReader` wrapped around `file.take(...)`.
- `tail_start<R: Read + Seek>(file, window) -> io::Result<u64>`
- `numbered_lines<R: BufRead>(reader) -> impl Iterator<Item = (usize, io::Result<String>)>`.
  Numbers are 1-based. Blank lines are skipped but still counted.

**`src/history.rs`**:

- `Record::to_json` and `Record::from_json(v) -> Result<Option<Record>, RecordError>`.
  `Ok(None)` means "a type this crate doesn't write", which is not an error.
- `HistoryWriter::append`: assemble the whole line, then make **one**
  `write_all` call.
- `read_day<R: BufRead>(reader) -> DayFile`. It never fails as a whole. Keep
  going past a bad line. An `InvalidData` read error is a bad line; any other
  I/O error is recorded, and then reading stops.

## You write the tests: `tests/t04_tail.rs`

Write them first. The file lists names and intent. Drive `Tail` with a
`Cursor<Vec<u8>>` you append to between polls (a helper is sketched in the
file), plus one test against a real `File` in `common::scratch_dir`. The
behaviours worth pinning down are:

- a partial line held across **several** polls, and the offset still
  advancing past it
- truncation: the old partial must not prefix the new first line
- `\r\n`
- blank lines, which are skipped but not counted as skipped
- invalid UTF-8, which is skipped and counted
- an oversized line **spanning two polls**, which is skipped whole and counted once
- `tail_start` when the window starts exactly on a line boundary, starts
  mid-line, holds no newline at all, or covers the whole file

## Katas

1. `BufReader::new(file.take(n))` compiles when `file: &mut R` and
   `R: Read`. Which impl makes a `&mut R` a reader? After the `BufReader` is
   dropped, can you use `file` again? Why?
2. `std::io::stdin().lines()` and `BufReader::new(File::open(p)?).lines()`
   both work, but `File::open(p)?.lines()` does not. What is `BufReader`
   adding that `lines` needs? And what goes wrong performance-wise if you
   hand-roll byte-at-a-time `read` on a `File`?
3. `HistoryWriter::new(&mut file)` in a loop, then `HistoryWriter::new(file)`
   once at the end. What does each form do to `file`'s ownership?

## Done when

`t04_history` and your `t04_tail` pass. Grading runs the hidden
`t04_tail_golden`, which has 17 tests.
