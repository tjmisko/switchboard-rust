//! T04 — the history day file: `Write` out, `BufRead` in.

mod common;

use std::fs::File;
use std::io::{BufReader, Cursor, Write};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

use common::{fixture, key};
use switchboard::error::RecordError;
use switchboard::history::{HistoryWriter, Record, RecordKind, day_path, read_day};
use switchboard::model::{HistoryEntry, Phase, SessionId};
use switchboard::time::parse_rfc3339;

fn at(s: &str) -> SystemTime {
    parse_rfc3339(s).unwrap()
}

fn shown(from: Phase, to: Phase) -> Record {
    Record {
        at: at("2026-09-11T15:36:35.500Z"),
        pid: 4036,
        session: Some(SessionId::from("s-1")),
        kind: RecordKind::Shown {
            from,
            to,
            held: Duration::from_millis(6352),
        },
    }
}

#[test]
fn should_write_one_json_object_per_line_into_any_writer() {
    let mut w = HistoryWriter::new(Vec::new());
    w.append(&shown(Phase::Unknown, Phase::Working)).unwrap();
    w.append(&shown(Phase::Working, Phase::Attention)).unwrap();
    let bytes = w.into_inner();
    let text = String::from_utf8(bytes).unwrap();

    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    assert!(text.ends_with('\n'));
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["type"], "shown");
    assert_eq!(first["from"], "unknown");
    assert_eq!(first["to"], "working");
    assert_eq!(first["held_ms"], 6352);
    assert_eq!(first["pid"], 4036);
    assert_eq!(first["session_id"], "s-1");
    assert_eq!(first["ts"], "2026-09-11T15:36:35.500Z");
}

#[test]
fn should_omit_session_id_when_the_record_has_none() {
    let r = Record {
        at: at("2026-09-11T08:36:29.148Z"),
        pid: 1,
        session: None,
        kind: RecordKind::SessionStart {
            cwd: PathBuf::from("/p"),
        },
    };
    let v = r.to_json();
    assert!(v.get("session_id").is_none());
    assert_eq!(v["cwd"], "/p");
}

#[test]
fn should_round_trip_every_record_kind_through_a_buffer() {
    let records = vec![
        Record {
            at: at("2026-09-11T08:36:29.148Z"),
            pid: 4036,
            session: None,
            kind: RecordKind::SessionStart {
                cwd: PathBuf::from("/home/u/Projects/Arachne"),
            },
        },
        shown(Phase::Idle, Phase::Stopped),
        Record {
            at: at("2026-09-11T15:43:00.000Z"),
            pid: 4036,
            session: Some("s-1".into()),
            kind: RecordKind::SessionEnd,
        },
    ];
    let mut w = HistoryWriter::new(Vec::new());
    for r in &records {
        w.append(r).unwrap();
    }
    let day = read_day(Cursor::new(w.into_inner()));
    assert!(day.errors.is_empty(), "{:?}", day.errors);
    assert_eq!(day.records, records);
}

#[test]
fn should_skip_foreign_lines_and_report_bad_ones_with_line_numbers() {
    let file = File::open(fixture("history/mixed.jsonl")).unwrap();
    let day = read_day(BufReader::new(file));

    assert_eq!(day.records.len(), 4);
    assert_eq!(day.foreign, 3, "activity, focus, transition");

    let numbers: Vec<usize> = day.errors.iter().map(|(n, _)| *n).collect();
    assert_eq!(numbers, [8, 9, 10, 11, 13]);

    let kinds: Vec<&RecordError> = day.errors.iter().map(|(_, e)| e).collect();
    assert!(matches!(kinds[0], RecordError::Name(n) if &*n.value == "permission"));
    assert!(matches!(kinds[1], RecordError::Missing("pid")));
    assert!(matches!(kinds[2], RecordError::Json(_)));
    assert!(matches!(
        kinds[3],
        RecordError::BadValue { field: "ts", .. }
    ));
    assert!(matches!(kinds[4], RecordError::Missing("type")));
}

#[test]
fn should_read_the_same_instant_from_z_and_offset_timestamps() {
    let file = File::open(fixture("history/mixed.jsonl")).unwrap();
    let day = read_day(BufReader::new(file));
    // line 6 is written "08:40:00.000-07:00", which is 15:40Z
    let attention = day
        .records
        .iter()
        .find(|r| {
            matches!(
                r.kind,
                RecordKind::Shown {
                    to: Phase::Attention,
                    ..
                }
            )
        })
        .unwrap();
    assert_eq!(attention.at, at("2026-09-11T15:40:00Z"));
}

#[test]
fn should_keep_reading_after_a_line_of_invalid_utf8() {
    let bytes = b"{\"ts\":\"2026-09-11T15:43:00Z\",\"type\":\"session_end\",\"pid\":1}\n\xff\xfe\n{\"ts\":\"2026-09-11T15:44:00Z\",\"type\":\"session_end\",\"pid\":2}\n";
    let day = read_day(Cursor::new(bytes.to_vec()));
    assert_eq!(day.records.len(), 2);
    assert_eq!(day.errors.len(), 1);
    assert_eq!(day.errors[0].0, 2);
    assert!(matches!(day.errors[0].1, RecordError::Io(_)));
}

#[test]
fn should_stamp_a_reducer_entry_with_the_wall_clock() {
    let entry = HistoryEntry::Shown {
        process: key(77),
        session: Some("s".into()),
        from: Phase::Working,
        to: Phase::Idle,
        held: Duration::from_secs(3),
    };
    let when = at("2026-09-11T00:00:00Z");
    let r = Record::from_entry(&entry, when);
    assert_eq!(r.pid, 77);
    assert_eq!(r.at, when);
    assert_eq!(r.type_name(), "shown");
}

#[test]
fn should_append_to_a_real_file_through_a_mutable_reference() {
    let dir = common::scratch_dir("history_append");
    let path = day_path(&dir, at("2026-09-11T23:59:59Z"));
    assert!(path.ends_with("2026-09-11.jsonl"));

    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .unwrap();
    for _ in 0..2 {
        // `&mut File` is itself `Write`: the writer borrows, the file survives.
        let mut w = HistoryWriter::new(&mut file);
        w.append(&shown(Phase::Idle, Phase::Working)).unwrap();
    }
    // ...and is still ours to write to directly.
    file.write_all(b"\n").unwrap();
    HistoryWriter::new(file)
        .append(&shown(Phase::Working, Phase::Idle))
        .unwrap();
    let day = read_day(BufReader::new(File::open(&path).unwrap()));
    assert_eq!(day.records.len(), 3);
}
