# T04 — Line-oriented I/O: tail and history

**Core: 1h30** · Gaps: `Read` / `BufRead` / `Write`, generic `R: BufRead`,
why `BufReader` exists, `Cursor<Vec<u8>>` and `Vec<u8>` as test doubles,
`&mut R` as a reader · Tests: `cargo test --test t04_history`, and
**`t04_tail`, which you write**

The transcript is a file another process appends to. The history is a file
this process appends to. Both are one JSON object per line. Nothing in this
task knows about JSON or transcripts: `Tail` yields `String`s, `HistoryWriter`
takes `Record`s, and neither cares what the underlying reader or writer is.

Do the history side first. It is fully specified by `t04_history` and
`tests/fixtures/history/mixed.jsonl`, and it gets the `Write`/`BufRead`
generics comfortable on a small scale before `Tail::poll`, which is the
hardest single function in T01 to T04 and the first that holds state across
calls.

## Implement

**`src/history.rs`** first:

- `Record::to_json` and `Record::from_json(v) -> Result<Option<Record>, RecordError>`.
  `Ok(None)` means "a type this crate doesn't write", which is not an error.
  The mixed fixture pins the check order: `type` before `ts` before `pid`.
- `HistoryWriter::append`: assemble the whole line, then make **one**
  `write_all` call.
- `read_day<R: BufRead>(reader) -> DayFile`. It never fails as a whole. Keep
  going past a bad line. An `InvalidData` read error is a bad line; any other
  I/O error is recorded, and then reading stops.

**`src/tail.rs`** second:

- `numbered_lines<R: BufRead>(reader) -> impl Iterator<Item = (usize, io::Result<String>)>`.
  Numbers are 1-based. Blank and whitespace-only lines are skipped but still
  counted. Write this one before `poll`; `read_day` needs it.
- `tail_start<R: Read + Seek>(file, window) -> io::Result<u64>`. The doc
  comment now states the answer for every case; the list below repeats it.
- `Tail::poll<R: Read + Seek>(&mut self, file: &mut R) -> io::Result<Polled>`.
  Read the module comment's hazards before you start: partial lines,
  shrinking files, oversized or non-UTF-8 lines, and a multibyte character
  split across polls. Use `read_until(b'\n')` on a `BufReader` wrapped
  around `file.take(...)`.

## Facts you need that nothing else states

- `Seek::seek` returns the new position, so `seek(SeekFrom::End(0))` is how
  `poll` and `tail_start` learn the file length.
- `read_until` includes the delimiter in the buffer, and at end of input
  returns whatever fragment was left with no delimiter. That fragment is the
  whole partial-line mechanism: it goes into the carry.
- `BufRead::lines` consumes the bytes of an invalid line and *keeps going*
  after yielding the `InvalidData` error. `numbered_lines`, `read_day` and
  the provided test `should_keep_reading_after_a_line_of_invalid_utf8` all
  depend on that.
- A multibyte character can straddle two polls (`caf\xc3` now, `\xa9\n`
  later). Carry bytes, not a `String`, and decode only once the `\n` has
  arrived; then this costs nothing.

## You write the tests: `tests/t04_tail.rs`

Write them first. The file lists names and intent. Drive `Tail` with a
`Cursor<Vec<u8>>` you append to between polls (a helper is sketched in the
file), plus one test against a real `File` in `common::scratch_dir`.

Every expected offset is a byte count. Derive it from the data with the
file's `len(b"...")` helper, not by counting: in the dry run every failing
test was a miscounted expectation, and the message looks exactly like a
broken `poll`.

The behaviours worth pinning down, with the answer golden gives for each:

- a partial line held across **several** polls, and the offset still
  advancing past it: the offset counts the carried bytes, so after
  `poll` on `ab` the offset is 2 and nothing has been returned
- truncation: the old partial must not prefix the new first line, and a
  skip in progress ends too. Only a *shorter* file is a truncation; a
  rewrite to the same length is not detected
- `\r\n`: the `\r` is stripped with the `\n`
- blank lines, which are skipped but not counted as skipped. Whitespace-only
  lines are blank (`trim().is_empty()`)
- invalid UTF-8, which is skipped and counted
- an oversized line **spanning two polls**, which is skipped whole and counted
  once, in the poll where the carried bytes first exceed `max_line`. That
  can be before its `\n` arrives; the later polls that discard the rest
  report nothing for it
- `max_line` measures the line *including* its terminator: with a limit of
  8, `1234567\n` is kept and `12345678\n` is skipped
- `tail_start`: window covering the whole file, or an empty file: `0`.
  Window starting exactly on a line boundary: that boundary. Window
  starting mid-line: the byte after the next `\n`. Window holding no `\n`
  at all: the file length, because nothing in it is a whole line

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
`t04_tail_golden`, which has 20 tests; every behaviour it checks is in the
list above.
