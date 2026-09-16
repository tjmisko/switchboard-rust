//! Adversarial audit: read every real Go-daemon history day file through
//! `history::read_day`.
//!
//! Run: `cargo test --release --test adv_history -- --ignored --nocapture`

use std::collections::BTreeMap;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

use serde_json::Value;
use switchboard::history::{HistoryWriter, Record, read_day};

#[test]
#[ignore]
fn real_history_files() {
    let dir = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join(".local/state/switchboard/history");
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "jsonl"))
        .collect();
    paths.sort();

    let mut total_bytes = 0u64;
    let mut total_records = 0usize;
    let mut total_foreign = 0usize;
    let mut total_errors = 0usize;
    let mut error_kinds: BTreeMap<String, usize> = BTreeMap::new();
    let mut error_examples: Vec<String> = Vec::new();
    let mut panics = 0;
    let started = Instant::now();
    let mut slowest = (std::time::Duration::ZERO, String::new(), 0u64);

    for path in &paths {
        let len = std::fs::metadata(path).unwrap().len();
        total_bytes += len;
        let t = Instant::now();
        let p = path.clone();
        let day = match std::panic::catch_unwind(|| {
            read_day(BufReader::new(File::open(&p).unwrap()))
        }) {
            Ok(d) => d,
            Err(_) => {
                panics += 1;
                println!("PANIC reading {}", path.display());
                continue;
            }
        };
        let took = t.elapsed();
        if took > slowest.0 {
            slowest = (took, path.file_name().unwrap().to_string_lossy().into(), len);
        }
        total_records += day.records.len();
        total_foreign += day.foreign;
        total_errors += day.errors.len();
        for (n, e) in &day.errors {
            let k: String = e.to_string().chars().take(50).collect();
            *error_kinds.entry(k.clone()).or_default() += 1;
            if error_examples.len() < 15 {
                error_examples.push(format!(
                    "{}:{n}: {}",
                    path.file_name().unwrap().to_string_lossy(),
                    k
                ));
            }
        }
        // Round trip: every record we read must re-serialise and re-read equal.
        for r in &day.records {
            let mut w = HistoryWriter::new(Vec::new());
            w.append(r).unwrap();
            let v: Value = serde_json::from_slice(&w.into_inner()).unwrap();
            let back = Record::from_json(&v).unwrap().unwrap();
            // `ts` is written at millisecond precision; the Go daemon wrote nanos.
            if back.kind != r.kind || back.pid != r.pid || back.session != r.session {
                println!("round-trip mismatch: {r:?} vs {back:?}");
            }
        }
    }
    let elapsed = started.elapsed();
    let mb = total_bytes as f64 / 1048576.0;
    println!(
        "files={} bytes={:.1}MiB records={} foreign={} errors={} panics={}",
        paths.len(),
        mb,
        total_records,
        total_foreign,
        total_errors,
        panics
    );
    println!(
        "wall {:?} => {:.2} ms/MiB; slowest {} {:?} for {:.1} MiB",
        elapsed,
        elapsed.as_secs_f64() * 1000.0 / mb,
        slowest.1,
        slowest.0,
        slowest.2 as f64 / 1048576.0
    );
    for (k, n) in &error_kinds {
        println!("{n:6} {k}");
    }
    for e in &error_examples {
        println!("{e}");
    }
    assert_eq!(panics, 0);
}
