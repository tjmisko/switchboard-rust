//! Adversarial audit: replay every real transcript under
//! `~/.claude/projects/*/*.jsonl` through `Tail` -> `ingest::claude::line` ->
//! `reduce::apply` -> `tick`, and report what looks wrong.
//!
//! Run: `cargo test --release --test adv_corpus -- --ignored --nocapture`
//!
//! Private content is never printed beyond 60 characters of a raw line.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;

use switchboard::ingest::Input;
use switchboard::ingest::claude::{LineCtx, line};
use switchboard::model::*;
use switchboard::paths::cwd_slug;
use switchboard::reduce::{self, default_account, next_deadline, tick};
use switchboard::tail::{Tail, tail_start};
use switchboard::time::parse_rfc3339;

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("HOME"))
}

fn transcripts() -> Vec<PathBuf> {
    let mut out = Vec::new();
    let root = home().join(".claude/projects");
    for dir in std::fs::read_dir(&root).expect("projects dir").flatten() {
        if !dir.path().is_dir() {
            continue;
        }
        for f in std::fs::read_dir(dir.path()).into_iter().flatten().flatten() {
            let p = f.path();
            if p.extension().is_some_and(|e| e == "jsonl") && p.is_file() {
                out.push(p);
            }
        }
    }
    out.sort();
    out
}

fn trunc(s: &str) -> String {
    s.chars().take(60).collect()
}

const PREFIXES: [&str; 4] = [
    "<local-command",
    "<command-",
    "<bash-",
    "<user-memory-input",
];

fn check_effects(effects: &[Effect], where_: &str, bad: &mut Vec<String>) {
    let publishes = effects
        .iter()
        .filter(|e| matches!(e, Effect::PublishState))
        .count();
    if publishes > 1 || (publishes == 1 && effects.last() != Some(&Effect::PublishState)) {
        bad.push(format!("effect order violated at {where_}: {effects:?}"));
    }
}

#[derive(Default, Debug)]
struct Stats {
    files: usize,
    bytes: u64,
    lines: usize,
    parse_errors: usize,
    tail_skipped: usize,
    inputs: usize,
    turn_started_human: usize,
    turn_started_machine: usize,
    human_string_prompts: usize,
    human_array_prompts_missed: usize,
    turn_ended_completed: usize,
    turn_ended_interrupted: usize,
    turn_ended_failed: usize,
    interrupt_marker_without_field: usize,
    dispatched: usize,
    settled: usize,
    settled_without_dispatch: usize,
    dispatched_never_settled: usize,
    dup_dispatch_ids: usize,
    limits_snapshots: usize,
    limits_rejected: usize,
    open_tools_at_eof: usize,
    files_with_open_tools_at_eof: usize,
    slug_mismatch: usize,
    cwd_changes_in_file: usize,
    last_line_bigger_than_window: usize,
    replay_window_no_lines: usize,
    replay_vs_full_truth_diverged: usize,
    final_truth: BTreeMap<&'static str, usize>,
    final_truth_after_stale: BTreeMap<&'static str, usize>,
    final_shown: BTreeMap<&'static str, usize>,
    publish_order_violations: usize,
    panics: usize,
}

struct Replayed {
    truth: Phase,
    truth_after_stale: Phase,
    shown: Phase,
    open_tools: usize,
}

/// Replay `lines` (already split) into a fresh board. Returns final state.
fn replay(
    session: &str,
    lines: &[String],
    stats: Option<&mut Stats>,
    offenders: &mut Vec<String>,
    name: &str,
) -> Replayed {
    let t0 = Instant::now();
    let key = ProcKey { pid: 4242, start: 1 };
    let mut board = Board::default();
    reduce::apply(
        &mut board,
        Input::Snapshot(Snapshot {
            taken: t0,
            kind: SnapshotKind::Processes(vec![ProcessRow {
                key,
                provider: Provider::Claude,
                cwd: PathBuf::from("/home/u/p"),
                session: Some(SessionId::from(session)),
                tty: None,
                cpu_ticks: 0,
            }]),
        }),
    );

    let mut local = Stats::default();
    let st = match stats {
        Some(s) => s,
        None => &mut local,
    };

    let mut first_claimed: Option<std::time::SystemTime> = None;
    let mut now = t0 + Duration::from_millis(1);
    let mut dispatched_ids: BTreeSet<String> = BTreeSet::new();
    let mut settled_ids: BTreeSet<String> = BTreeSet::new();
    let mut last_cwd: Option<String> = None;

    for (i, text) in lines.iter().enumerate() {
        st.lines += 1;
        let Ok(v) = serde_json::from_str::<Value>(text) else {
            st.parse_errors += 1;
            offenders.push(format!("{name}:{} parse error: {}", i + 1, trunc(text)));
            continue;
        };
        if let Some(cwd) = v.get("cwd").and_then(Value::as_str) {
            if last_cwd.as_deref().is_some_and(|c| c != cwd) {
                st.cwd_changes_in_file += 1;
            }
            last_cwd = Some(cwd.to_owned());
        }
        // Drive a monotonic clock from claimed timestamps.
        if let Some(claimed) = v
            .get("timestamp")
            .and_then(Value::as_str)
            .and_then(parse_rfc3339)
        {
            let base = *first_claimed.get_or_insert(claimed);
            if let Ok(d) = claimed.duration_since(base) {
                let cand = t0 + Duration::from_millis(1) + d;
                if cand > now {
                    now = cand;
                }
            }
        }

        // Independent oracle for human prompts.
        let is_user = v.get("type").and_then(Value::as_str) == Some("user");
        let side = v.get("isSidechain").and_then(Value::as_bool) == Some(true);
        let meta = v.get("isMeta").and_then(Value::as_bool) == Some(true);
        if is_user && !side && !meta && v.get("interruptedMessageId").is_none() {
            let content = v.get("message").and_then(|m| m.get("content"));
            let origin_human = v
                .get("origin")
                .and_then(|o| o.get("kind"))
                .and_then(Value::as_str)
                .is_none_or(|k| k == "human");
            match content {
                Some(Value::String(s)) if !PREFIXES.iter().any(|p| s.starts_with(p)) => {
                    if origin_human {
                        st.human_string_prompts += 1;
                    }
                }
                Some(Value::Array(blocks)) => {
                    let has_result = blocks
                        .iter()
                        .any(|b| b.get("type").and_then(Value::as_str) == Some("tool_result"));
                    let has_text = blocks
                        .iter()
                        .any(|b| b.get("type").and_then(Value::as_str) == Some("text"));
                    let marker = blocks.iter().any(|b| {
                        b.get("text")
                            .and_then(Value::as_str)
                            .is_some_and(|t| t.starts_with("[Request interrupted by user"))
                    });
                    if marker {
                        st.interrupt_marker_without_field += 1;
                        offenders.push(format!(
                            "{name}:{} interrupt marker w/o interruptedMessageId",
                            i + 1
                        ));
                    } else if !has_result && has_text && origin_human {
                        st.human_array_prompts_missed += 1;
                        if st.human_array_prompts_missed <= 5 {
                            offenders.push(format!(
                                "{name}:{} human prompt with array content yields no TurnStarted: {}",
                                i + 1,
                                trunc(text)
                            ));
                        }
                    }
                }
                _ => {}
            }
        }

        let ctx = LineCtx {
            process: key,
            session: SessionId::from(session),
            account: default_account(),
            now,
            confidence: Confidence::Confirmed,
        };
        let inputs = line(&v, &ctx);
        for input in inputs {
            st.inputs += 1;
            match &input {
                Input::Event(e) => match &e.kind {
                    EventKind::TurnStarted { origin, .. } => {
                        if *origin == Origin::Human {
                            st.turn_started_human += 1
                        } else {
                            st.turn_started_machine += 1
                        }
                    }
                    EventKind::TurnEnded { outcome, .. } => match outcome {
                        TurnOutcome::Completed => st.turn_ended_completed += 1,
                        TurnOutcome::Interrupted { .. } => st.turn_ended_interrupted += 1,
                        TurnOutcome::Failed { .. } => st.turn_ended_failed += 1,
                    },
                    EventKind::ToolDispatched { call, .. } => {
                        st.dispatched += 1;
                        if !dispatched_ids.insert(call.to_string()) {
                            st.dup_dispatch_ids += 1;
                        }
                    }
                    EventKind::ToolSettled { call, .. } => {
                        st.settled += 1;
                        if !dispatched_ids.contains(call.as_str()) {
                            st.settled_without_dispatch += 1;
                        }
                        settled_ids.insert(call.to_string());
                    }
                    _ => {}
                },
                Input::Snapshot(s) => {
                    if let SnapshotKind::Limits { value, .. } = &s.kind {
                        st.limits_snapshots += 1;
                        if value.status == LimitStatus::Rejected {
                            st.limits_rejected += 1;
                        }
                    }
                }
            }
            // Realistic: the core ticks whenever a deadline passes.
            if let Some(d) = next_deadline(&board)
                && d <= now
            {
                let fx = tick(&mut board, now);
                check_effects(&fx, &format!("{name}:{} tick", i + 1), offenders);
            }
            let fx = reduce::apply(&mut board, input);
            let before = offenders.len();
            check_effects(&fx, &format!("{name}:{}", i + 1), offenders);
            st.publish_order_violations += offenders.len() - before;
        }
    }

    st.dispatched_never_settled += dispatched_ids.difference(&settled_ids).count();

    let fx = tick(&mut board, now);
    check_effects(&fx, &format!("{name}: final tick"), offenders);
    let a = board.agents.get(&key).expect("agent survives");
    let open_tools = match &a.activity {
        Activity::Turn { tools, .. } => tools.len(),
        _ => 0,
    };
    let truth = board.phase(a);
    let shown = a.shown.phase;
    let later = now + Duration::from_secs(3600);
    tick(&mut board, later);
    let a = board.agents.get(&key).expect("agent survives");
    let truth_after_stale = board.phase(a);
    Replayed {
        truth,
        truth_after_stale,
        shown,
        open_tools,
    }
}

#[test]
#[ignore]
fn corpus_replay() {
    let files = transcripts();
    let mut stats = Stats::default();
    let mut offenders: Vec<String> = Vec::new();
    let started = Instant::now();
    let mut per_file: Vec<(Duration, u64, String)> = Vec::new();

    for path in &files {
        let name = format!(
            "{}/{}",
            path.parent()
                .and_then(Path::file_name)
                .map(|s| s.to_string_lossy().chars().take(24).collect::<String>())
                .unwrap_or_default(),
            path.file_name().unwrap().to_string_lossy()
        );
        let session = path.file_stem().unwrap().to_string_lossy().to_string();
        let t = Instant::now();
        let mut file = File::open(path).unwrap();
        let len = file.metadata().unwrap().len();
        let polled = Tail::new().poll(&mut file).unwrap();
        stats.files += 1;
        stats.bytes += len;
        stats.tail_skipped += polled.skipped;

        // Slug check against the first cwd.
        let first_cwd = polled.lines.iter().find_map(|l| {
            serde_json::from_str::<Value>(l)
                .ok()?
                .get("cwd")?
                .as_str()
                .map(str::to_owned)
        });
        if let Some(cwd) = &first_cwd {
            let dir = path.parent().unwrap().file_name().unwrap().to_string_lossy();
            if cwd_slug(Path::new(cwd)) != dir {
                stats.slug_mismatch += 1;
                offenders.push(format!(
                    "{name}: slug mismatch: cwd_slug={} dir={}",
                    trunc(&cwd_slug(Path::new(cwd))),
                    trunc(&dir)
                ));
            }
        }

        let lines = polled.lines.clone();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut local_off = Vec::new();
            let r = replay(&session, &lines, Some(&mut stats), &mut local_off, &name);
            (r, local_off)
        }));
        let (full, local_off) = match result {
            Ok(x) => x,
            Err(_) => {
                stats.panics += 1;
                offenders.push(format!("{name}: PANIC"));
                continue;
            }
        };
        offenders.extend(local_off);
        if full.open_tools > 0 {
            stats.open_tools_at_eof += full.open_tools;
            stats.files_with_open_tools_at_eof += 1;
        }
        *stats.final_truth.entry(full.truth.as_str()).or_default() += 1;
        *stats
            .final_truth_after_stale
            .entry(full.truth_after_stale.as_str())
            .or_default() += 1;
        *stats.final_shown.entry(full.shown.as_str()).or_default() += 1;

        // What discovery actually does: replay only the last 128 KiB.
        let window = 128 * 1024;
        let start = tail_start(&mut file, window).unwrap();
        if len > window && start >= len {
            stats.replay_window_no_lines += 1;
        }
        let mut tail = Tail::starting_at(start);
        let windowed = tail.poll(&mut file).unwrap();
        if len > window && windowed.lines.is_empty() {
            stats.last_line_bigger_than_window += 1;
            offenders.push(format!(
                "{name}: last 128KiB holds no complete line; replay yields nothing (len={len})"
            ));
        }
        if len > window {
            let mut ignore = Vec::new();
            let w = replay(&session, &windowed.lines, None, &mut ignore, &name);
            if w.truth != full.truth {
                stats.replay_vs_full_truth_diverged += 1;
                offenders.push(format!(
                    "{name}: windowed replay truth {:?} != full replay truth {:?}",
                    w.truth, full.truth
                ));
            }
        }
        per_file.push((t.elapsed(), len, name));
    }

    let elapsed = started.elapsed();
    let mb = stats.bytes as f64 / (1024.0 * 1024.0);
    println!("\n==== corpus replay ====");
    println!("{stats:#?}");
    println!(
        "wall {:?} for {:.1} MiB => {:.2} ms/MiB (includes second windowed replay)",
        elapsed,
        mb,
        elapsed.as_secs_f64() * 1000.0 / mb
    );
    per_file.sort_by(|a, b| b.0.cmp(&a.0));
    for (d, len, name) in per_file.iter().take(5) {
        println!(
            "slowest: {name} {:?} {:.2} MiB => {:.2} ms/MiB",
            d,
            *len as f64 / 1048576.0,
            d.as_secs_f64() * 1000.0 / (*len as f64 / 1048576.0)
        );
    }
    println!("---- offenders ({}) ----", offenders.len());
    let mut by_kind: BTreeMap<String, usize> = BTreeMap::new();
    for o in &offenders {
        let kind = o.split_once(": ").map(|(_, k)| k).unwrap_or(o);
        let kind: String = kind.chars().take(40).collect();
        *by_kind.entry(kind).or_default() += 1;
    }
    for o in offenders.iter().take(60) {
        println!("{o}");
    }
    assert_eq!(stats.panics, 0);
}

/// Why does a file end Blocked / Working / Unknown? Structural info only.
#[test]
#[ignore]
fn corpus_final_phase_reasons() {
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut examples: Vec<String> = Vec::new();
    for path in transcripts() {
        let session = path.file_stem().unwrap().to_string_lossy().to_string();
        let mut file = File::open(&path).unwrap();
        let polled = Tail::new().poll(&mut file).unwrap();
        let t0 = Instant::now();
        let key = ProcKey { pid: 1, start: 1 };
        let mut board = Board::default();
        reduce::apply(
            &mut board,
            Input::Snapshot(Snapshot {
                taken: t0,
                kind: SnapshotKind::Processes(vec![ProcessRow {
                    key,
                    provider: Provider::Claude,
                    cwd: "/x".into(),
                    session: Some(SessionId::from(session.as_str())),
                    tty: None,
                    cpu_ticks: 0,
                }]),
            }),
        );
        let mut last_kind = String::from("none");
        let mut last_claimed = None;
        let mut rejected_at = None;
        let mut resets_at = None;
        let mut n_inputs = 0;
        let mut now = t0;
        for (i, text) in polled.lines.iter().enumerate() {
            let Ok(v) = serde_json::from_str::<Value>(text) else { continue };
            now += Duration::from_millis(1);
            if let Some(c) = v.get("timestamp").and_then(Value::as_str).and_then(parse_rfc3339) {
                last_claimed = Some(c);
            }
            let ctx = LineCtx {
                process: key,
                session: SessionId::from(session.as_str()),
                account: default_account(),
                now,
                confidence: Confidence::Confirmed,
            };
            for input in line(&v, &ctx) {
                n_inputs += 1;
                last_kind = match &input {
                    Input::Event(e) => format!("{:?}", e.kind).chars().take(40).collect(),
                    Input::Snapshot(s) => {
                        if let SnapshotKind::Limits { value, .. } = &s.kind
                            && value.status == LimitStatus::Rejected
                        {
                            rejected_at = last_claimed;
                            resets_at = value.resets_at;
                            let _ = i;
                        }
                        "Snapshot".into()
                    }
                };
                reduce::apply(&mut board, input);
            }
        }
        tick(&mut board, now);
        let a = &board.agents[&key];
        let truth = board.phase(a);
        let limited = reduce::limits_for(&board.accounts, a)
            .is_some_and(|l| l.status == LimitStatus::Rejected);
        let reason = match truth {
            Phase::Blocked if limited => {
                let stale_h = match (rejected_at, resets_at, last_claimed) {
                    (Some(_), Some(r), Some(last)) => last
                        .duration_since(r)
                        .map(|d| format!("{}h after resetsAt", d.as_secs() / 3600))
                        .unwrap_or_else(|_| "before resetsAt".into()),
                    _ => "?".into(),
                };
                let after = match &a.activity {
                    Activity::Idle { after, .. } => format!("{after:?}").chars().take(20).collect::<String>(),
                    Activity::Turn { .. } => "Turn".into(),
                    Activity::Unobserved { .. } => "Unobserved".into(),
                };
                examples.push(format!("{session}: Blocked by account limit; EOF is {stale_h}; activity after={after}"));
                "blocked: account limit Rejected (never expires)".to_string()
            }
            Phase::Blocked => "blocked: last turn Failed".to_string(),
            Phase::Working => format!("working: last input {last_kind}"),
            Phase::Unknown => format!("unknown: inputs={n_inputs} lines={}", polled.lines.len().min(5)),
            p => format!("{}", p.as_str()),
        };
        *reasons.entry(reason).or_default() += 1;
    }
    for (r, n) in &reasons {
        println!("{n:5}  {r}");
    }
    for e in examples.iter().take(30) {
        println!("{e}");
    }
}
