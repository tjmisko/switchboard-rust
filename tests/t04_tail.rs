//! T04 — YOUR tests for `Tail`, `tail_start` and `numbered_lines`.
//!
//! Write each body before implementing, and watch it fail. The list below is
//! not complete. Read the three hazards in the `tail.rs` module comment and add
//! at least three tests for cases it doesn't name. The hidden golden suite has
//! several.

mod common;

use std::fs::{File, OpenOptions};
use std::io::{BufReader, Cursor, Seek, SeekFrom, Write};

use switchboard::tail::{Polled, Tail, numbered_lines, tail_start};

/// Append to a `Cursor` without moving its read position, the way another
/// process appends to a file you are tailing.
fn append(file: &mut Cursor<Vec<u8>>, bytes: &[u8]) {
    let pos = file.position();
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(bytes).unwrap();
    file.set_position(pos);
}

/// Replace the whole contents, as a rewrite or truncation would.
fn rewrite(file: &mut Cursor<Vec<u8>>, bytes: &[u8]) {
    *file = Cursor::new(bytes.to_vec());
}

fn cursor(bytes: &[u8]) -> Cursor<Vec<u8>> {
    Cursor::new(bytes.to_vec())
}

fn lines(polled: &Polled) -> Vec<&str> {
    polled.lines.iter().map(String::as_str).collect()
}

// ---------------------------------------------------------------------------
// Tail::poll
// ---------------------------------------------------------------------------

/// Complete lines come back in order, without their newlines. The offset ends
/// at the end of the data, and nothing is reported as truncated or skipped.
#[test]
fn should_return_every_complete_line_in_order() {
    let mut file = cursor(b"one\ntwo\nthree\n");
    let mut tail = Tail::new();

    let polled = tail.poll(&mut file).unwrap();

    assert_eq!(lines(&polled), ["one", "two", "three"]);
    assert!(!polled.truncated);
    assert_eq!(polled.skipped, 0);
    assert_eq!(tail.offset(), 14);

    // Nothing new: an empty poll, and the offset stays put.
    let again = tail.poll(&mut file).unwrap();
    assert_eq!(again, Polled::default());
    assert_eq!(tail.offset(), 14);
}

/// A line whose `\n` has not arrived is not returned. Keep appending fragments
/// over **several** polls, then the newline, and get exactly one whole line.
#[test]
fn should_hold_a_partial_line_across_several_polls() {
    let mut file = cursor(b"done\n{\"a\":");
    let mut tail = Tail::new();

    let first = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&first), ["done"]);
    assert_eq!(tail.offset(), 10, "the offset covers the carried fragment too");

    append(&mut file, b"1,\"b\"");
    assert!(tail.poll(&mut file).unwrap().lines.is_empty());
    assert_eq!(tail.offset(), 15);

    append(&mut file, b":2}");
    assert!(tail.poll(&mut file).unwrap().lines.is_empty());

    append(&mut file, b"\nnext");
    let polled = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&polled), [r#"{"a":1,"b":2}"#]);
    assert_eq!(polled.skipped, 0);

    append(&mut file, b"\n");
    assert_eq!(lines(&tail.poll(&mut file).unwrap()), ["next"]);
    assert_eq!(tail.offset(), 24);
}

/// The file shrank. The next poll starts again from zero, says `truncated`,
/// and whatever partial line was being carried must not end up glued to the
/// front of the new data.
#[test]
fn should_restart_from_zero_and_drop_the_carry_when_the_file_shrinks() {
    let mut file = cursor(b"old-1\nold-2\nold-par");
    let mut tail = Tail::new();
    let first = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&first), ["old-1", "old-2"]);
    assert_eq!(tail.offset(), 19);

    rewrite(&mut file, b"new-1\n");
    let polled = tail.poll(&mut file).unwrap();
    assert!(polled.truncated);
    assert_eq!(lines(&polled), ["new-1"], "no `old-par` prefix");
    assert_eq!(tail.offset(), 6);

    // Life goes on from the new offset.
    append(&mut file, b"new-2\n");
    let next = tail.poll(&mut file).unwrap();
    assert!(!next.truncated);
    assert_eq!(lines(&next), ["new-2"]);
}

/// `\r\n` line endings yield lines without the `\r`.
#[test]
fn should_strip_a_trailing_carriage_return() {
    let mut file = cursor(b"a\r\nb\r\nc\n");
    let polled = Tail::new().poll(&mut file).unwrap();
    assert_eq!(lines(&polled), ["a", "b", "c"]);
}

/// A line that is not UTF-8 is dropped and counted in `skipped`, and the
/// lines around it still come back.
#[test]
fn should_skip_and_count_a_line_that_is_not_utf8() {
    let mut file = cursor(b"ok-1\n\xff\xfe bad\nok-2\n");
    let mut tail = Tail::new();
    let polled = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&polled), ["ok-1", "ok-2"]);
    assert_eq!(polled.skipped, 1);
    assert_eq!(tail.offset(), 17, "the bad line is consumed, not re-read");
}

/// `Tail::starting_at(n)` resumes from a saved byte offset.
#[test]
fn should_resume_from_a_saved_offset() {
    let data = b"first\nsecond\nthird\n";
    let mut file = cursor(data);
    let mut tail = Tail::starting_at(6);
    let polled = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&polled), ["second", "third"]);
    assert!(!polled.truncated);
    assert_eq!(tail.offset(), data.len() as u64);

    // An offset saved from one tail carries over to a fresh one.
    let mut resumed = Tail::starting_at(tail.offset());
    append(&mut file, b"fourth\n");
    assert_eq!(lines(&resumed.poll(&mut file).unwrap()), ["fourth"]);
}

/// One real `File`, written through a second handle opened for append, polled
/// twice. Use `common::scratch_dir`, not `/tmp`.
#[test]
fn should_follow_a_real_file_being_appended_to() {
    let dir = common::scratch_dir("tail_real_file");
    let path = dir.join("transcript.jsonl");
    let mut writer = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    writer.write_all(b"{\"n\":1}\n{\"n\":2}\n{\"n\":").unwrap();

    let mut reader = File::open(&path).unwrap();
    let mut tail = Tail::new();
    let first = tail.poll(&mut reader).unwrap();
    assert_eq!(lines(&first), [r#"{"n":1}"#, r#"{"n":2}"#]);

    writer.write_all(b"3}\n").unwrap();
    let second = tail.poll(&mut reader).unwrap();
    assert_eq!(lines(&second), [r#"{"n":3}"#]);
    assert!(!second.truncated);
    assert_eq!(tail.offset(), 24);
}

// ---------------------------------------------------------------------------
// tail_start
// ---------------------------------------------------------------------------

/// A window as large as the file, or larger, starts at zero. So does an empty
/// file.
#[test]
fn should_start_at_zero_when_the_window_covers_the_file() {
    let data = b"aaa\nbbb\n";
    assert_eq!(tail_start(&mut cursor(data), 8).unwrap(), 0);
    assert_eq!(tail_start(&mut cursor(data), 1000).unwrap(), 0);
    assert_eq!(tail_start(&mut cursor(b""), 0).unwrap(), 0);
    assert_eq!(tail_start(&mut cursor(b""), 64).unwrap(), 0);
}

/// A window that begins exactly at the start of a line keeps that line.
#[test]
fn should_start_at_the_window_edge_when_it_falls_exactly_on_a_line_start() {
    // "aaa\n" | "bbb\n" | "cc\n": a window of 7 starts right at "bbb".
    let data = b"aaa\nbbb\ncc\n";
    assert_eq!(tail_start(&mut cursor(data), 7).unwrap(), 4);
    // A window of 3 starts right at "cc".
    assert_eq!(tail_start(&mut cursor(data), 3).unwrap(), 8);
}

/// A window that begins mid-line skips the rest of that line. Try several
/// window sizes that land inside the same line.
#[test]
fn should_skip_the_partial_first_line_when_the_window_starts_mid_line() {
    let data = b"aaa\nbbbbbb\ncc\n";
    // Windows of 4..=9 all start somewhere inside "bbbbbb\n" and must land
    // on "cc" at offset 11.
    for window in 4..=9 {
        assert_eq!(tail_start(&mut cursor(data), window).unwrap(), 11, "window {window}");
    }
    // A window of 1 or 2 starts inside "cc\n": nothing complete follows.
    for window in 1..=2 {
        assert_eq!(
            tail_start(&mut cursor(data), window).unwrap(),
            14,
            "window {window} holds no line start"
        );
    }
}

/// A window that holds no newline at all, because the file's only line is
/// still being written, starts at the end: nothing in it is a whole line.
#[test]
fn should_start_at_the_end_when_the_window_holds_no_newline() {
    let data = b"aaa\nbbbbbbbbbb";
    assert_eq!(tail_start(&mut cursor(data), 5).unwrap(), 14);
}

/// `tail_start` leaves the reader usable: the offset it returns is exactly
/// where a `Tail` should begin.
#[test]
fn should_hand_a_tail_an_offset_it_can_resume_from() {
    let mut file = cursor(b"a\nbb\nccc\n");
    let start = tail_start(&mut file, 5).unwrap();
    let polled = Tail::starting_at(start).poll(&mut file).unwrap();
    assert_eq!(lines(&polled), ["ccc"]);
}

// ---------------------------------------------------------------------------
// numbered_lines
// ---------------------------------------------------------------------------

/// Numbers start at 1. Blank and whitespace-only lines are not yielded, but
/// they still take up a number.
#[test]
fn should_number_lines_from_one_and_keep_numbers_across_blank_lines() {
    let reader = BufReader::new(cursor(b"one\n\ntwo\n   \n\t\nthree"));
    let got: Vec<(usize, String)> = numbered_lines(reader)
        .map(|(n, l)| (n, l.unwrap()))
        .collect();
    assert_eq!(
        got,
        [(1, "one".to_string()), (3, "two".to_string()), (6, "three".to_string())]
    );
}

/// Invalid UTF-8 surfaces as an `InvalidData` error at its own number, and
/// the lines after it are still yielded.
#[test]
fn should_yield_an_invalid_data_error_for_a_non_utf8_line_and_continue() {
    let reader = BufReader::new(cursor(b"good\n\xff\xfe\nalso good\n"));
    let got: Vec<(usize, std::io::Result<String>)> = numbered_lines(reader).collect();
    assert_eq!(got.len(), 3);
    assert_eq!(got[0].0, 1);
    assert_eq!(got[1].0, 2);
    assert_eq!(
        got[1].1.as_ref().unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(got[2].1.as_ref().unwrap(), "also good");
}

// ---------------------------------------------------------------------------
// yours: at least three more
// ---------------------------------------------------------------------------

/// A line longer than `max_line` is skipped whole and counted once, and the
/// lines on either side of it still arrive.
#[test]
fn should_skip_and_count_an_oversized_line_once_when_it_arrives_in_one_poll() {
    let mut file = cursor(b"short\n0123456789abcdef\nafter\n");
    let mut tail = Tail::new().with_max_line(8);
    let polled = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&polled), ["short", "after"]);
    assert_eq!(polled.skipped, 1);
    assert_eq!(tail.offset(), 29);
}

/// An oversized line that arrives in pieces across two polls is skipped whole
/// and counted exactly once, and the line after it still arrives.
#[test]
fn should_skip_an_oversized_line_spanning_two_polls_and_count_it_once() {
    let mut file = cursor(b"short\n0123456789");
    let mut tail = Tail::new().with_max_line(8);

    let first = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&first), ["short"]);

    append(&mut file, b"abcdef");
    let second = tail.poll(&mut file).unwrap();
    assert!(second.lines.is_empty());

    append(&mut file, b"ghij\nafter\n");
    let third = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&third), ["after"]);

    let total_skipped = first.skipped + second.skipped + third.skipped;
    assert_eq!(total_skipped, 1, "one oversized line, counted once");
    assert_eq!(tail.offset(), 33);
}

/// A line of exactly `max_line` bytes is not oversized.
#[test]
fn should_keep_a_line_that_is_exactly_max_line_long() {
    let mut file = cursor(b"12345678\n123456789\n");
    let polled = Tail::new().with_max_line(8).poll(&mut file).unwrap();
    assert_eq!(lines(&polled), ["12345678"]);
    assert_eq!(polled.skipped, 1);
}

/// Blank lines are neither returned nor counted as skipped, but the offset
/// still moves past them.
#[test]
fn should_skip_blank_lines_without_counting_them() {
    let mut file = cursor(b"a\n\n\r\n   \nb\n");
    let mut tail = Tail::new();
    let polled = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&polled), ["a", "b"]);
    assert_eq!(polled.skipped, 0);
    assert_eq!(tail.offset(), 11);
}

/// Truncation while an oversized line is being skipped ends the skip: the
/// new file's first line must not be swallowed by it.
#[test]
fn should_stop_skipping_when_the_file_shrinks_mid_oversized_line() {
    let mut file = cursor(b"0123456789abcdef");
    let mut tail = Tail::new().with_max_line(8);
    let first = tail.poll(&mut file).unwrap();
    assert_eq!(first.skipped, 1);

    rewrite(&mut file, b"tiny\n");
    let polled = tail.poll(&mut file).unwrap();
    assert!(polled.truncated);
    assert_eq!(lines(&polled), ["tiny"]);
    assert_eq!(polled.skipped, 0);
}

/// A file that is rewritten to the same length or longer, keeping our offset
/// valid, is not detected as truncated: only shrinking is.
#[test]
fn should_not_report_truncation_when_the_file_only_grows() {
    let mut file = cursor(b"a\n");
    let mut tail = Tail::new();
    tail.poll(&mut file).unwrap();
    append(&mut file, b"b\n");
    let polled = tail.poll(&mut file).unwrap();
    assert!(!polled.truncated);
    assert_eq!(lines(&polled), ["b"]);
}

/// A partial line that is not UTF-8 so far may still complete into a valid
/// line, because a multibyte character can straddle two polls.
#[test]
fn should_reassemble_a_multibyte_character_split_across_polls() {
    // "é" is 0xC3 0xA9.
    let mut file = cursor(b"caf\xc3");
    let mut tail = Tail::new();
    assert!(tail.poll(&mut file).unwrap().lines.is_empty());
    append(&mut file, b"\xa9\n");
    let polled = tail.poll(&mut file).unwrap();
    assert_eq!(lines(&polled), ["café"]);
    assert_eq!(polled.skipped, 0);
}
