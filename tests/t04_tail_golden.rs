//! T04 (golden, hidden) — following an appended file.
//! The learner writes their own version of these in `t04_tail.rs`.

mod common;

use std::fs::OpenOptions;
use std::io::{BufReader, Cursor, Seek, SeekFrom, Write};

use switchboard::tail::{Tail, numbered_lines, tail_start};

/// A `Cursor` that can be appended to between polls, like a file being
/// written by another process.
fn append(file: &mut Cursor<Vec<u8>>, bytes: &[u8]) {
    let pos = file.position();
    file.seek(SeekFrom::End(0)).unwrap();
    file.write_all(bytes).unwrap();
    file.set_position(pos);
}

#[test]
fn should_return_every_complete_line_in_order() {
    let mut f = Cursor::new(b"a\nb\nc\n".to_vec());
    let mut t = Tail::new();
    let p = t.poll(&mut f).unwrap();
    assert_eq!(p.lines, ["a", "b", "c"]);
    assert_eq!(t.offset(), 6);
    assert!(!p.truncated);
    assert_eq!(p.skipped, 0);
}

#[test]
fn should_return_nothing_when_nothing_was_appended() {
    let mut f = Cursor::new(b"a\n".to_vec());
    let mut t = Tail::new();
    t.poll(&mut f).unwrap();
    assert!(t.poll(&mut f).unwrap().lines.is_empty());
}

#[test]
fn should_hold_a_partial_line_across_several_polls() {
    let mut f = Cursor::new(b"{\"type\":".to_vec());
    let mut t = Tail::new();
    assert!(t.poll(&mut f).unwrap().lines.is_empty());
    append(&mut f, b"\"user\"");
    assert!(t.poll(&mut f).unwrap().lines.is_empty());
    append(&mut f, b"}\nnext");
    assert_eq!(t.poll(&mut f).unwrap().lines, [r#"{"type":"user"}"#]);
    append(&mut f, b"\n");
    assert_eq!(t.poll(&mut f).unwrap().lines, ["next"]);
}

#[test]
fn should_advance_the_offset_past_a_partial_line_without_rereading_it() {
    let mut f = Cursor::new(b"ab".to_vec());
    let mut t = Tail::new();
    t.poll(&mut f).unwrap();
    assert_eq!(t.offset(), 2, "the carried bytes are consumed");
    append(&mut f, b"c\n");
    assert_eq!(t.poll(&mut f).unwrap().lines, ["abc"]);
}

#[test]
fn should_restart_from_zero_and_drop_the_carry_when_the_file_shrinks() {
    let mut t = Tail::new();
    t.poll(&mut Cursor::new(b"old line\npartial".to_vec()))
        .unwrap();
    let p = t.poll(&mut Cursor::new(b"new\n".to_vec())).unwrap();
    assert!(p.truncated);
    assert_eq!(
        p.lines,
        ["new"],
        "the old partial must not prefix the new line"
    );
    assert_eq!(t.offset(), 4);
}

#[test]
fn should_not_report_truncation_when_the_file_is_the_same_length() {
    let mut t = Tail::new();
    t.poll(&mut Cursor::new(b"abc\n".to_vec())).unwrap();
    let p = t.poll(&mut Cursor::new(b"xyz\n".to_vec())).unwrap();
    assert!(!p.truncated);
    assert!(p.lines.is_empty());
}

#[test]
fn should_strip_a_trailing_carriage_return() {
    let mut f = Cursor::new(b"crlf\r\nlf\n".to_vec());
    assert_eq!(Tail::new().poll(&mut f).unwrap().lines, ["crlf", "lf"]);
}

#[test]
fn should_skip_blank_lines_without_counting_them() {
    let mut f = Cursor::new(b"\n  \nx\n\n".to_vec());
    let p = Tail::new().poll(&mut f).unwrap();
    assert_eq!(p.lines, ["x"]);
    assert_eq!(p.skipped, 0);
}

#[test]
fn should_skip_and_count_a_line_that_is_not_utf8() {
    let mut f = Cursor::new(b"ok\n\xff\xfe\nalso ok\n".to_vec());
    let p = Tail::new().poll(&mut f).unwrap();
    assert_eq!(p.lines, ["ok", "also ok"]);
    assert_eq!(p.skipped, 1);
}

#[test]
fn should_skip_an_oversized_line_whole_even_when_it_spans_polls() {
    let mut f = Cursor::new(b"short\n0123456789".to_vec());
    let mut t = Tail::new().with_max_line(8);
    let p = t.poll(&mut f).unwrap();
    assert_eq!(p.lines, ["short"]);
    assert_eq!(p.skipped, 1);

    append(&mut f, b"abcdef\nafter\n");
    let p = t.poll(&mut f).unwrap();
    assert_eq!(
        p.lines,
        ["after"],
        "the tail of the long line is discarded too"
    );
    assert_eq!(p.skipped, 0, "one long line is counted once");
}

/// The limit measures the line including its terminator: with `max_line = 8`,
/// `1234567\n` is eight bytes and kept; `12345678\n` is nine and skipped.
#[test]
fn should_count_the_terminator_when_measuring_a_line_against_max_line() {
    let mut f = Cursor::new(b"1234567\n12345678\nok\n".to_vec());
    let p = Tail::new().with_max_line(8).poll(&mut f).unwrap();
    assert_eq!(p.lines, ["1234567", "ok"]);
    assert_eq!(p.skipped, 1);
}

/// The file shrinks while the rest of an oversized line is being discarded.
/// The skip ends with the truncation, so the new file's first line arrives.
#[test]
fn should_stop_skipping_when_the_file_shrinks_mid_oversized_line() {
    let mut t = Tail::new().with_max_line(8);
    let p = t
        .poll(&mut Cursor::new(b"0123456789abcdef".to_vec()))
        .unwrap();
    assert_eq!(p.skipped, 1);
    let p = t.poll(&mut Cursor::new(b"tiny\n".to_vec())).unwrap();
    assert!(p.truncated);
    assert_eq!(p.lines, ["tiny"]);
    assert_eq!(p.skipped, 0);
}

/// `é` is `\xc3\xa9`. Its two bytes can land in different polls, and the
/// line must still decode: the carry is bytes, and decoding waits for `\n`.
#[test]
fn should_reassemble_a_multibyte_character_split_across_polls() {
    let mut f = Cursor::new(b"caf\xc3".to_vec());
    let mut t = Tail::new();
    let p = t.poll(&mut f).unwrap();
    assert!(p.lines.is_empty());
    assert_eq!(p.skipped, 0, "an incomplete line is not yet invalid");
    append(&mut f, b"\xa9\n");
    let p = t.poll(&mut f).unwrap();
    assert_eq!(p.lines, ["café"]);
    assert_eq!(p.skipped, 0);
}

#[test]
fn should_resume_from_a_saved_offset() {
    let mut f = Cursor::new(b"one\ntwo\nthree\n".to_vec());
    let mut t = Tail::starting_at(4);
    assert_eq!(t.poll(&mut f).unwrap().lines, ["two", "three"]);
}

#[test]
fn should_follow_a_real_file_being_appended_to() {
    let dir = common::scratch_dir("tail_real_file");
    let path = dir.join("t.jsonl");
    std::fs::write(&path, b"first\nsec").unwrap();

    let mut reader = std::fs::File::open(&path).unwrap();
    let mut t = Tail::new();
    assert_eq!(t.poll(&mut reader).unwrap().lines, ["first"]);

    let mut writer = OpenOptions::new().append(true).open(&path).unwrap();
    writer.write_all(b"ond\nthird\n").unwrap();
    assert_eq!(t.poll(&mut reader).unwrap().lines, ["second", "third"]);
}

#[test]
fn should_start_at_zero_when_the_window_covers_the_file() {
    let mut f = Cursor::new(b"a\nb\n".to_vec());
    assert_eq!(tail_start(&mut f, 4).unwrap(), 0);
    assert_eq!(tail_start(&mut f, 100).unwrap(), 0);
    assert_eq!(tail_start(&mut Cursor::new(Vec::new()), 10).unwrap(), 0);
}

#[test]
fn should_start_at_the_window_edge_when_it_falls_exactly_on_a_line_start() {
    let data = b"0123456789\nabc\ndef\n";
    let mut f = Cursor::new(data.to_vec());
    let off = tail_start(&mut f, 8).unwrap() as usize;
    assert_eq!(&data[off..], b"abc\ndef\n");
}

#[test]
fn should_skip_the_partial_first_line_when_the_window_starts_mid_line() {
    let data = b"0123456789\nabc\ndef\n";
    let mut f = Cursor::new(data.to_vec());
    for window in [5, 6, 7] {
        let off = tail_start(&mut f, window).unwrap() as usize;
        assert_eq!(&data[off..], b"def\n", "window {window}");
    }
}

#[test]
fn should_return_the_end_when_the_window_holds_no_line_start() {
    let data = b"aaaaaaaaaaaaaaaaaaaa";
    let mut f = Cursor::new(data.to_vec());
    assert_eq!(tail_start(&mut f, 5).unwrap(), data.len() as u64);
}

#[test]
fn should_number_lines_from_one_and_keep_numbers_across_blank_lines() {
    let text = b"a\n\nb\n   \nc";
    let got: Vec<(usize, String)> = numbered_lines(BufReader::new(&text[..]))
        .map(|(n, l)| (n, l.unwrap()))
        .collect();
    assert_eq!(
        got,
        [
            (1, "a".to_string()),
            (3, "b".to_string()),
            (5, "c".to_string())
        ]
    );
}
