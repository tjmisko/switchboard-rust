//! Adversarial audit: randomised reducer sequences with invariant checks.
//!
//! Hand-rolled SplitMix64, no crates. Every violation category is counted and
//! the first seed + step that hit it is printed so it can be minimised.
//!
//! Run: `cargo test --release --test adv_props -- --ignored --nocapture`
//! Seeds: `ADV_SEEDS=20000 ADV_STEPS=300`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

use switchboard::ingest::Input;
use switchboard::model::*;
use switchboard::reduce::{self, next_deadline, tick};
use switchboard::view::View;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<'a, T>(&mut self, xs: &'a [T]) -> &'a T {
        &xs[self.below(xs.len() as u64) as usize]
    }
    fn chance(&mut self, pct: u64) -> bool {
        self.below(100) < pct
    }
}

#[derive(Clone, Copy)]
struct Cfg {
    /// Owners drawn for gates. The "unknown-only" run never draws Human.
    owners: &'static [Ownership],
}

fn clone_board(b: &Board) -> Board {
    Board {
        agents: b.agents.clone(),
        accounts: b.accounts.clone(),
    }
}

fn row(pid: u32, start: u64, session: Option<&str>) -> ProcessRow {
    ProcessRow {
        key: ProcKey { pid, start },
        provider: Provider::Claude,
        cwd: PathBuf::from(format!("/home/u/p{pid}")),
        session: session.map(SessionId::from),
        tty: None,
        cpu_ticks: 0,
    }
}

fn gen_event(r: &mut Rng, cfg: Cfg, now: Instant, starts: &BTreeMap<u32, u64>) -> Event {
    let pid = 1 + r.below(3) as u32;
    let start = *starts.get(&pid).unwrap_or(&1);
    let session = r.pick(&["s1", "s2", "s3", "s1b", "s2b"]).to_string();
    let call = ToolUseId::from(*r.pick(&["c1", "c2", "c3", "c4"]));
    let tool = ToolName::from(*r.pick(&["Bash", "Read", "AskUserQuestion"]));
    let hash = InputHash([r.below(3) as u8; 8]);
    let prompt = TurnKey::Prompt(PromptId::from(*r.pick(&["p1", "p2", "p3"])));
    let sub = SubagentId::from(*r.pick(&["a1", "a2"]));
    let actor = if r.chance(20) {
        Actor::Subagent(sub.clone())
    } else {
        Actor::Agent
    };
    let owner = *r.pick(cfg.owners);
    let kind = match r.below(17) {
        0 => EventKind::SessionStarted {
            source: *r.pick(&[
                StartSource::Startup,
                StartSource::Resume,
                StartSource::Clear,
                StartSource::Compact,
            ]),
            title: None,
        },
        1 => EventKind::SessionEnded {
            reason: "other".into(),
        },
        2 | 3 => EventKind::TurnStarted {
            turn: r.chance(80).then(|| prompt.clone()),
            origin: *r.pick(&[Origin::Human, Origin::TaskNotification]),
        },
        4 => EventKind::TurnEnded {
            turn: r.chance(50).then(|| prompt.clone()),
            outcome: r
                .pick(&[
                    TurnOutcome::Completed,
                    TurnOutcome::Interrupted { killed: None },
                    TurnOutcome::Failed {
                        class: ErrorClass::RateLimit,
                    },
                ])
                .clone(),
        },
        5 | 6 => EventKind::ToolDispatched { call, tool, hash },
        7 | 8 => EventKind::ToolSettled {
            call,
            outcome: r
                .pick(&[
                    ToolOutcome::Ok { ms: None },
                    ToolOutcome::Failed { interrupt: false },
                    ToolOutcome::Failed { interrupt: true },
                    ToolOutcome::Denied { by: Denier::User },
                ])
                .clone(),
        },
        9 | 10 => EventKind::PermissionAsked {
            call: r.chance(40).then_some(call),
            tool,
            hash: r.chance(80).then_some(hash),
            owner,
            kind: *r.pick(&[GateKind::Approval, GateKind::Question]),
        },
        11 => EventKind::PermissionSettled {
            call: r.chance(60).then_some(call),
            req: None,
            outcome: *r.pick(&[
                Decision::Allowed,
                Decision::Denied { by: Denier::User },
                Decision::Abandoned,
            ]),
        },
        12 => EventKind::SubagentStarted {
            subagent: sub,
            kind: SubagentKind::Task,
        },
        13 => EventKind::SubagentEnded {
            subagent: sub,
            outcome: SubagentOutcome::Completed,
        },
        14 => EventKind::CompactionStarted {
            trigger: Trigger::Auto,
        },
        15 => EventKind::CompactionEnded,
        _ => EventKind::CwdChanged {
            to: PathBuf::from(*r.pick(&["/a", "/b"])),
        },
    };
    Event {
        at: Moment {
            local: now,
            claimed: None,
        },
        process: r.chance(70).then_some(ProcKey { pid, start }),
        session: SessionId::from(session),
        actor,
        cause: if r.chance(30) { Cause::User } else { Cause::Agent },
        confidence: *r.pick(&[Confidence::Confirmed, Confidence::Inferred, Confidence::Snapshot]),
        kind,
    }
}

fn gen_snapshot(r: &mut Rng, now: Instant, starts: &BTreeMap<u32, u64>) -> Snapshot {
    let session = SessionId::from(*r.pick(&["s1", "s2", "s3", "s1b"]));
    let kind = match r.below(5) {
        0 => {
            let mut rows = Vec::new();
            for pid in 1..=3u32 {
                if !r.chance(90) {
                    continue;
                }
                let s = format!("s{pid}");
                let named = r.chance(70);
                rows.push(row(pid, starts[&pid], named.then_some(s.as_str())));
            }
            SnapshotKind::Processes(rows)
        }
        1 => SnapshotKind::Limits {
            account: reduce::default_account(),
            value: Limits {
                status: *r.pick(&[LimitStatus::Allowed, LimitStatus::Warning, LimitStatus::Rejected]),
                resets_at: Some(SystemTime::UNIX_EPOCH),
                window: None,
                used_percent: None,
            },
        },
        2 => SnapshotKind::Waits {
            session,
            rows: (0..r.below(3))
                .map(|i| WaitRow {
                    id: WaitId(format!("w{i}").into()),
                    kind: WaitKind::BackgroundShell,
                })
                .collect(),
        },
        3 => SnapshotKind::Subagents {
            session,
            rows: (0..r.below(3))
                .map(|i| SubagentRow {
                    subagent: SubagentId::from(format!("a{i}")),
                    kind: SubagentKind::Task,
                    status: *r.pick(&[SubagentStatus::Running, SubagentStatus::Completed]),
                    started: SystemTime::UNIX_EPOCH,
                    label: "x".into(),
                    tokens: 0,
                })
                .collect(),
        },
        _ => SnapshotKind::Beat {
            session,
            value: Beat {
                model: "m".into(),
                context_tokens: 1,
                api_ms: r.below(100),
            },
        },
    };
    // Snapshots may be taken a little before they are applied.
    let taken = now.checked_sub(Duration::from_millis(r.below(3000))).unwrap_or(now);
    Snapshot { taken, kind }
}

#[derive(Default)]
struct Report {
    counts: BTreeMap<String, usize>,
    first: BTreeMap<String, String>,
    max_unjoined: usize,
}

impl Report {
    fn hit(&mut self, cat: &str, detail: impl FnOnce() -> String) {
        *self.counts.entry(cat.to_string()).or_default() += 1;
        if !self.first.contains_key(cat) {
            self.first.insert(cat.to_string(), detail());
        }
    }
}

fn check_effects(fx: &[Effect]) -> bool {
    let n = fx.iter().filter(|e| matches!(e, Effect::PublishState)).count();
    n <= 1 && (n == 0 || fx.last() == Some(&Effect::PublishState))
}

fn kind_name(k: &EventKind) -> String {
    format!("{k:?}")
        .split([' ', '{', '('])
        .next()
        .unwrap()
        .to_string()
}

fn run(seed: u64, steps: usize, cfg: Cfg, rep: &mut Report) {
    let mut r = Rng(seed);
    let t0 = Instant::now();
    let mut now = t0;
    let mut starts: BTreeMap<u32, u64> = (1..=3).map(|p| (p, 1)).collect();
    let mut board = Board::default();
    let mut ever_attention: BTreeSet<ProcKey> = BTreeSet::new();
    let mut trace: Vec<String> = Vec::new();

    reduce::apply_snapshot(
        &mut board,
        Snapshot {
            taken: now,
            kind: SnapshotKind::Processes(vec![
                row(1, 1, Some("s1")),
                row(2, 1, None),
                row(3, 1, Some("s3")),
            ]),
        },
    );

    for step in 0..steps {
        // Time moves: mostly small, sometimes past a dwell or an expiry.
        let dt = match r.below(20) {
            0 => Duration::from_secs(11 * 60),
            1 => Duration::from_secs(31 * 60),
            2..=4 => Duration::from_secs(11),
            _ => Duration::from_millis(r.below(3000)),
        };
        now += dt;

        // The core ticks whenever the board's deadline has passed.
        if let Some(d) = next_deadline(&board)
            && d <= now
        {
            let fx = tick(&mut board, now);
            if !check_effects(&fx) {
                rep.hit("effects: PublishState not single/last (tick)", || format!("{fx:?}"));
            }
            if let Some(d2) = next_deadline(&board)
                && d2 <= now
            {
                rep.hit("next_deadline <= now right after one tick (extra core wakeup)", || {
                    format!("seed={seed} step={step}")
                });
                let mut spins = 0;
                while next_deadline(&board).is_some_and(|d| d <= now) && spins < 10 {
                    tick(&mut board, now);
                    spins += 1;
                }
                if spins >= 10 {
                    rep.hit("next_deadline never clears (core spins forever)", || {
                        format!("seed={seed} step={step}")
                    });
                }
            }
        }

        let choice = r.below(10);
        let (fx, label) = if choice == 0 {
            // Death, then maybe a recycled pid with a new start time.
            let pid = 1 + r.below(3) as u32;
            let key = ProcKey {
                pid,
                start: starts[&pid],
            };
            let e = Event {
                at: Moment {
                    local: now,
                    claimed: None,
                },
                process: Some(key),
                session: SessionId::from(""),
                actor: Actor::Agent,
                cause: Cause::Agent,
                confidence: Confidence::Confirmed,
                kind: EventKind::Died,
            };
            let fx = reduce::apply_event(&mut board, e);
            if r.chance(50) {
                *starts.get_mut(&pid).unwrap() += 1;
            }
            ever_attention.remove(&key);
            (fx, format!("Died pid={pid}"))
        } else if choice <= 2 {
            let s = gen_snapshot(&mut r, now, &starts);
            let label = format!("{:?}", s.kind).chars().take(80).collect::<String>();
            (reduce::apply(&mut board, Input::Snapshot(s)), label)
        } else {
            let e = gen_event(&mut r, cfg, now, &starts);
            let label = format!("{:?}", e).chars().take(200).collect::<String>();

            // Duplicate application must be a no-op.
            let mut shadow = clone_board(&board);
            let _ = reduce::apply_event(&mut shadow, e.clone());
            let before = format!("{:?}", shadow);
            let fx2 = reduce::apply_event(&mut shadow, e.clone());
            let after = format!("{:?}", shadow);
            if !fx2.is_empty() {
                rep.hit(&format!("duplicate event produced effects: {}", kind_name(&e.kind)), || {
                    format!("seed={seed} step={step} event={label} effects={fx2:?}")
                });
            }
            if before != after {
                rep.hit(&format!("duplicate event changed state: {}", kind_name(&e.kind)), || {
                    format!("seed={seed} step={step} event={label}")
                });
            }
            (reduce::apply_event(&mut board, e), label)
        };
        trace.push(label);

        if !check_effects(&fx) {
            rep.hit("effects: PublishState not single/last", || format!("{fx:?}"));
        }
        for e in &fx {
            if let Effect::AppendHistory(HistoryEntry::Shown { from, to, .. }) = e
                && from == to
            {
                rep.hit("history Shown with from == to", || format!("{e:?}"));
            }
        }

        // Render must not panic.
        let view = View::render(&board);
        let _ = view.to_json(now, SystemTime::now());

        if let Some(d) = next_deadline(&board)
            && d < now
        {
            rep.hit("next_deadline in the past after apply", || {
                format!("seed={seed} step={step} last={}", trace.last().unwrap())
            });
        }

        for (key, agent) in &board.agents {
            let truth = board.phase(agent);
            if truth == Phase::Attention {
                ever_attention.insert(*key);
            }
            if agent.shown.phase == Phase::Attention && !ever_attention.contains(key) {
                rep.hit("shown Attention without truth ever Attention", || {
                    format!("seed={seed} step={step}")
                });
            }
            if !cfg.owners.contains(&Ownership::Human)
                && (truth == Phase::Attention || agent.shown.phase == Phase::Attention)
            {
                rep.hit("Attention with no Human-owned gate ever issued", || {
                    format!("seed={seed} step={step} trace_tail={:?}", &trace[trace.len().saturating_sub(6)..])
                });
            }
            // shown must equal truth, or be serving a dwell toward it.
            let consistent = agent.shown.phase == truth
                || agent
                    .shown
                    .pending
                    .as_ref()
                    .is_some_and(|p| p.to == truth && p.promote_at > now);
            let unobserved_under_limit = matches!(agent.activity, Activity::Unobserved { .. })
                && agent.shown.phase == Phase::Unknown
                && truth == Phase::Blocked;
            if !consistent && unobserved_under_limit {
                rep.hit("discovered agent under a Rejected limit: shown Unknown, truth Blocked, never settled", || {
                    format!("seed={seed} step={step}")
                });
            }
            if !consistent && !unobserved_under_limit {
                let last_kind: String = trace.last().unwrap().chars().take(60).collect::<String>();
                let last_kind = if last_kind.starts_with("Event") {
                    trace.last().unwrap().split("kind: ").nth(1).unwrap_or("?").split([' ', '{', '(']).next().unwrap().to_string()
                } else {
                    last_kind.split([' ', '{', '(']).next().unwrap().to_string()
                };
                rep.hit(&format!("shown != truth, no pending toward truth; shown={:?} truth={truth:?} after {last_kind}", agent.shown.phase), || {
                    format!(
                        "seed={seed} step={step} shown={:?} truth={truth:?} pending={:?} trace_tail={:?}",
                        agent.shown.phase,
                        agent.shown.pending,
                        &trace[trace.len().saturating_sub(4)..]
                    )
                });
            }
            rep.max_unjoined = rep.max_unjoined.max(agent.prompts.unjoined.len());
        }
    }
}

fn env(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[test]
#[ignore]
fn reducer_properties() {
    let seeds = env("ADV_SEEDS", 5000) as u64;
    let steps = env("ADV_STEPS", 300);
    for (label, cfg) in [
        (
            "mixed owners",
            Cfg {
                owners: &[Ownership::Human, Ownership::Machine, Ownership::Unknown],
            },
        ),
        (
            "no human owners",
            Cfg {
                owners: &[Ownership::Machine, Ownership::Unknown],
            },
        ),
    ] {
        let mut rep = Report::default();
        let started = Instant::now();
        let mut panics = 0;
        for seed in 0..seeds {
            let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let mut local = Report::default();
                run(seed, steps, cfg, &mut local);
                local
            }));
            match res {
                Ok(local) => {
                    for (k, v) in local.counts {
                        *rep.counts.entry(k.clone()).or_default() += v;
                        rep.first.entry(k.clone()).or_insert_with(|| local.first[&k].clone());
                    }
                    rep.max_unjoined = rep.max_unjoined.max(local.max_unjoined);
                }
                Err(_) => {
                    panics += 1;
                    if panics < 3 {
                        println!("PANIC seed={seed}");
                    }
                }
            }
        }
        println!(
            "\n=== {label}: {seeds} seeds x {steps} steps in {:?}; panics={panics}; max unjoined={}",
            started.elapsed(),
            rep.max_unjoined
        );
        for (k, n) in &rep.counts {
            println!("{n:8}  {k}\n          first: {}", rep.first[k]);
        }
        assert_eq!(panics, 0);
    }
}
