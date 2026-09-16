//! T04 — YOUR tests for `Tail`, `tail_start` and `numbered_lines`.
//!
//! Write each body before implementing, and watch it fail. The list below is
//! not complete. Read the three hazards in the `tail.rs` module comment and add
//! at least three tests for cases it doesn't name. The hidden golden suite has
//! several.
//!
//! Every expected offset in this file is a byte count. Compute each one from
//! the data (`len(b"…")`, below) instead of counting by hand: a miscounted
//! expectation fails exactly like a wrong `poll`, and you cannot tell them
//! apart from the assertion message.

// Delete once the tests are written.
#![allow(unused)]

mod common;

use std::io::{BufReader, Cursor, Seek, SeekFrom, Write};

use switchboard::tail::{Tail, numbered_lines, tail_start};

/// Append to a `Cursor` without moving its read position, the way another
/// process appends to a file you are tailing.
fn append(file: &mut Cursor<Vec<u8>>, bytes: &[u8]) {
    let pos = file.position();
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(bytes).unwrap();
    file.set_position(pos);
}

/// The length of some bytes as an offset, so an expectation is derived from
/// the data it describes: `assert_eq!(t.offset(), len(b"one\ntw"))`.
fn len(bytes: &[u8]) -> u64 {
    bytes.len() as u64
}

// ---------------------------------------------------------------------------
// Tail::poll
// ---------------------------------------------------------------------------

/// Complete lines come back in order, without their newlines. The offset ends
/// at the end of the data, and nothing is reported as truncated or skipped.
#[test]
fn should_return_every_complete_line_in_order() {
    todo!("write this test")
}

/// A line whose `\n` has not arrived is not returned. Keep appending fragments
/// over **several** polls, then the newline, and get exactly one whole line.
#[test]
fn should_hold_a_partial_line_across_several_polls() {
    todo!("write this test")
}

/// The file shrank. The next poll starts again from zero, says `truncated`,
/// and whatever partial line was being carried must not end up glued to the
/// front of the new data.
#[test]
fn should_restart_from_zero_and_drop_the_carry_when_the_file_shrinks() {
    todo!("write this test")
}

/// `\r\n` line endings yield lines without the `\r`.
#[test]
fn should_strip_a_trailing_carriage_return() {
    todo!("write this test")
}

/// A line that is not UTF-8 is dropped and counted in `skipped`, and the
/// lines around it still come back.
#[test]
fn should_skip_and_count_a_line_that_is_not_utf8() {
    todo!("write this test")
}

/// `Tail::starting_at(n)` resumes from a saved byte offset.
#[test]
fn should_resume_from_a_saved_offset() {
    todo!("write this test")
}

/// One real `File`, written through a second handle opened for append, polled
/// twice. Use `common::scratch_dir`, not `/tmp`.
#[test]
fn should_follow_a_real_file_being_appended_to() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// tail_start
// ---------------------------------------------------------------------------

/// A window as large as the file, or larger, starts at zero. So does an empty
/// file.
#[test]
fn should_start_at_zero_when_the_window_covers_the_file() {
    todo!("write this test")
}

/// A window that begins exactly at the start of a line keeps that line.
#[test]
fn should_start_at_the_window_edge_when_it_falls_exactly_on_a_line_start() {
    todo!("write this test")
}

/// A window that begins mid-line skips the rest of that line. Try several
/// window sizes that land inside the same line.
#[test]
fn should_skip_the_partial_first_line_when_the_window_starts_mid_line() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// numbered_lines
// ---------------------------------------------------------------------------

/// Numbers start at 1. Blank and whitespace-only lines are not yielded, but
/// they still take up a number.
#[test]
fn should_number_lines_from_one_and_keep_numbers_across_blank_lines() {
    todo!("write this test")
}

// ---------------------------------------------------------------------------
// yours: at least three more
// ---------------------------------------------------------------------------
