//! Adversarial audit: `proc::sweep` against the live `/proc`.
//!
//! Run: `cargo test --release --test adv_proc -- --ignored --nocapture`

use std::path::{Path, PathBuf};
use std::time::Instant;

use switchboard::proc::{ancestors, parse_stat, sweep};

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap())
}

#[test]
#[ignore]
fn live_sweep_matches_pgrep() {
    let sessions = home().join(".claude/sessions");
    let t = Instant::now();
    let rows = sweep(Path::new("/proc"), &sessions);
    let took = t.elapsed();
    println!("sweep took {took:?}, found {} rows", rows.len());
    for r in &rows {
        println!(
            "  pid={} start={} cwd={} session={:?} tty={:?}",
            r.key.pid,
            r.key.start,
            r.cwd.display(),
            r.session.as_deref(),
            r.tty
        );
    }

    // Independent oracle: every process whose comm is `claude`, plus every
    // process whose argv[0] basename is `claude` (npm installs run under node).
    let mut oracle = Vec::new();
    for entry in std::fs::read_dir("/proc").unwrap().flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
            continue;
        };
        let dir = entry.path();
        let Ok(stat) = std::fs::read_to_string(dir.join("stat")) else {
            continue;
        };
        let Ok(s) = parse_stat(&stat) else {
            println!("parse_stat failed for live pid {pid}");
            continue;
        };
        let argv0 = std::fs::read(dir.join("cmdline"))
            .ok()
            .and_then(|b| {
                b.split(|&c| c == 0)
                    .next()
                    .map(|a| String::from_utf8_lossy(a).into_owned())
            })
            .unwrap_or_default();
        let base = Path::new(&argv0)
            .file_name()
            .map(|f| f.to_string_lossy().into_owned())
            .unwrap_or_default();
        if s.comm == "claude" || base == "claude" || argv0.contains("claude-code") {
            oracle.push((pid, s.comm.clone(), argv0));
        }
    }
    println!("oracle (comm or argv0 claude): {oracle:?}");
    let found: Vec<u32> = rows.iter().map(|r| r.key.pid).collect();
    for (pid, comm, argv0) in &oracle {
        if !found.contains(pid) {
            println!("FALSE NEGATIVE? pid={pid} comm={comm} argv0={argv0}");
        }
    }
    for pid in &found {
        if !oracle.iter().any(|(p, ..)| p == pid) {
            println!("FALSE POSITIVE? pid={pid}");
        }
    }

    // The test process runs under a claude agent's Bash tool: its ancestor
    // chain must reach a swept agent.
    let me = std::process::id();
    let chain: Vec<u32> = ancestors(Path::new("/proc"), me, 20).collect();
    println!("ancestors of test pid {me}: {chain:?}");
    let hit = chain.iter().find(|p| found.contains(p));
    println!("nearest tracked ancestor: {hit:?}");

    // Cost of a sweep at 1 Hz.
    let t = Instant::now();
    for _ in 0..20 {
        let _ = sweep(Path::new("/proc"), &sessions);
    }
    println!("mean sweep {:?}", t.elapsed() / 20);
}
