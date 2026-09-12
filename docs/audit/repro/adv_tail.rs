//! Adversarial audit: `Tail` against a real file appended by a concurrent
//! writer thread at high rate, with lines split across many `write` calls.
//!
//! Run: `cargo test --release --test adv_tail -- --nocapture`

mod common;

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use switchboard::tail::Tail;

struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.0 >> 33
    }
}

/// Line `i`: a sequence number, a length that varies wildly, and a checksum.
fn make_line(i: u64, rng: &mut Lcg, max_pad: u64) -> String {
    let pad_len = (rng.next() % max_pad) as usize;
    let pad: String = (0..pad_len)
        .map(|k| (b'a' + ((i as usize + k) % 26) as u8) as char)
        .collect();
    let sum = pad.bytes().fold(i, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
    format!("{i} {sum} {pad}")
}

fn check_line(line: &str) -> Option<u64> {
    let mut parts = line.splitn(3, ' ');
    let i: u64 = parts.next()?.parse().ok()?;
    let sum: u64 = parts.next()?.parse().ok()?;
    let pad = parts.next().unwrap_or("");
    let expect = pad.bytes().fold(i, |h, b| h.wrapping_mul(31).wrapping_add(u64::from(b)));
    (expect == sum).then_some(i)
}

fn run(name: &str, total: u64, max_pad: u64, max_line: usize, crlf: bool) -> (Vec<u64>, usize, Vec<u64>) {
    let dir = common::scratch_dir(name);
    let path = dir.join("t.jsonl");
    File::create(&path).unwrap();
    let done = Arc::new(AtomicBool::new(false));

    let writer = {
        let path = path.clone();
        let done = done.clone();
        thread::spawn(move || {
            let mut f = OpenOptions::new().append(true).open(&path).unwrap();
            let mut rng = Lcg(42);
            let mut oversized = Vec::new();
            for i in 0..total {
                let mut line = make_line(i, &mut rng, max_pad);
                if line.len() + if crlf { 2 } else { 1 } > max_line {
                    oversized.push(i);
                }
                line.push_str(if crlf { "\r\n" } else { "\n" });
                let bytes = line.as_bytes();
                // Split into 1..=4 writes at random points; sometimes yield
                // mid-line so the reader sees a fragment.
                let pieces = 1 + (rng.next() % 4) as usize;
                let mut start = 0;
                for p in 0..pieces {
                    let end = if p + 1 == pieces {
                        bytes.len()
                    } else {
                        (start + (rng.next() as usize % (bytes.len() - start + 1))).min(bytes.len())
                    };
                    f.write_all(&bytes[start..end]).unwrap();
                    start = end;
                    if rng.next() % 64 == 0 {
                        thread::yield_now();
                    }
                }
            }
            done.store(true, Ordering::SeqCst);
            oversized
        })
    };

    let mut file = File::open(&path).unwrap();
    let mut tail = Tail::new().with_max_line(max_line);
    let mut seen = Vec::with_capacity(total as usize);
    let mut skipped = 0;
    let mut bad = 0;
    let deadline = Instant::now() + Duration::from_secs(120);
    loop {
        let finished = done.load(Ordering::SeqCst);
        let polled = tail.poll(&mut file).unwrap();
        assert!(!polled.truncated);
        skipped += polled.skipped;
        for l in &polled.lines {
            match check_line(l) {
                Some(i) => seen.push(i),
                None => bad += 1,
            }
        }
        if finished && polled.lines.is_empty() && polled.skipped == 0 {
            // One more poll after the writer is done, to drain everything.
            let last = tail.poll(&mut file).unwrap();
            if last.lines.is_empty() && last.skipped == 0 {
                break;
            }
            skipped += last.skipped;
            for l in &last.lines {
                match check_line(l) {
                    Some(i) => seen.push(i),
                    None => bad += 1,
                }
            }
        }
        assert!(Instant::now() < deadline, "tail did not keep up");
    }
    let oversized = writer.join().unwrap();
    assert_eq!(bad, 0, "corrupted or spliced lines");
    (seen, skipped, oversized)
}

fn assert_exact(seen: &[u64], total: u64, excluded: &[u64]) {
    let mut expect = (0..total).filter(|i| !excluded.contains(i));
    for (k, &got) in seen.iter().enumerate() {
        let want = expect.next();
        assert_eq!(Some(got), want, "at position {k}: lost, duplicated or reordered");
    }
    assert_eq!(expect.next(), None, "lines lost at the end");
}

#[test]
fn should_deliver_every_line_once_in_order_when_a_writer_appends_concurrently() {
    let total = 300_000;
    let t = Instant::now();
    let (seen, skipped, oversized) = run("adv_tail_concurrent", total, 300, 1 << 20, false);
    println!("{} lines in {:?}", seen.len(), t.elapsed());
    assert!(oversized.is_empty());
    assert_eq!(skipped, 0);
    assert_exact(&seen, total, &[]);
}

#[test]
fn should_skip_exactly_the_oversized_lines_under_concurrent_appends() {
    let total = 50_000;
    let (seen, skipped, oversized) = run("adv_tail_oversized", total, 2_000, 1_000, false);
    println!("oversized={} skipped={skipped}", oversized.len());
    assert_eq!(skipped, oversized.len());
    assert_exact(&seen, total, &oversized);
}

#[test]
fn should_strip_crlf_under_concurrent_appends() {
    let total = 50_000;
    let (seen, skipped, _) = run("adv_tail_crlf", total, 200, 1 << 20, true);
    assert_eq!(skipped, 0);
    assert_exact(&seen, total, &[]);
}

/// A line that is exactly `max_line` bytes *including* its newline.
#[test]
fn should_agree_on_the_max_line_boundary() {
    use std::io::Cursor;
    let max = 10;
    // 9 bytes + '\n' = 10: allowed. 10 bytes + '\n' = 11: skipped.
    let data = b"123456789\n1234567890\nok\n".to_vec();
    let polled = Tail::new().with_max_line(max).poll(&mut Cursor::new(data)).unwrap();
    println!("boundary: {:?}", polled);
    assert_eq!(polled.lines, ["123456789", "ok"]);
    assert_eq!(polled.skipped, 1);
}
