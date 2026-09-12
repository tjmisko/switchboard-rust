//! Shared test harness. Provided code: builders so a reducer test reads as a
//! story, not as fifty lines of struct literals.
//!
//! ```ignore
//! let t = Clock::new();
//! let mut b = board_with(&[(100, "s1")], t.at(0));
//! apply(&mut b, hook(100, "s1", t.at(1), turn_started("p1")));
//! assert_eq!(truth(&b, 100), Phase::Working);
//! ```

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

use serde_json::Value;

use switchboard::ingest::Input;
use switchboard::ingest::hash::input_hash;
use switchboard::model::*;
use switchboard::reduce;

// ---------------------------------------------------------------------------
// Time
// ---------------------------------------------------------------------------

/// A fixed origin, so tests speak in offsets.
pub struct Clock {
    t0: Instant,
}

impl Clock {
    pub fn new() -> Clock {
        Clock { t0: Instant::now() }
    }

    pub fn at(&self, secs: u64) -> Instant {
        self.t0 + Duration::from_secs(secs)
    }

    pub fn at_ms(&self, ms: u64) -> Instant {
        self.t0 + Duration::from_millis(ms)
    }
}

// ---------------------------------------------------------------------------
// Boards
// ---------------------------------------------------------------------------

pub fn key(pid: u32) -> ProcKey {
    ProcKey { pid, start: 1000 }
}

pub fn row(pid: u32, session: Option<&str>) -> ProcessRow {
    ProcessRow {
        key: key(pid),
        provider: Provider::Claude,
        cwd: PathBuf::from(format!("/home/u/Projects/p{pid}")),
        session: session.map(SessionId::from),
        tty: Some(34816),
        cpu_ticks: 0,
    }
}

/// A board whose agents have been discovered by a sweep and have each sent
/// `SessionStart{startup}`: idle, observed, and showing idle.
pub fn board_with(agents: &[(u32, &str)], at: Instant) -> Board {
    let mut board = Board::default();
    let rows = agents.iter().map(|(pid, s)| row(*pid, Some(s))).collect();
    reduce::apply_snapshot(
        &mut board,
        Snapshot {
            taken: at,
            kind: SnapshotKind::Processes(rows),
        },
    );
    for (pid, session) in agents {
        apply(
            &mut board,
            hook(
                *pid,
                session,
                at,
                EventKind::SessionStarted {
                    source: StartSource::Startup,
                    title: None,
                },
            ),
        );
    }
    board
}

/// A board with agents discovered and never heard from.
pub fn board_unobserved(agents: &[(u32, Option<&str>)], at: Instant) -> Board {
    let mut board = Board::default();
    let rows = agents.iter().map(|(pid, s)| row(*pid, *s)).collect();
    reduce::apply_snapshot(
        &mut board,
        Snapshot {
            taken: at,
            kind: SnapshotKind::Processes(rows),
        },
    );
    board
}

pub fn apply(board: &mut Board, event: Event) -> Vec<Effect> {
    reduce::apply_event(board, event)
}

pub fn agent(board: &Board, pid: u32) -> &Agent {
    board
        .agents
        .get(&key(pid))
        .unwrap_or_else(|| panic!("no agent with pid {pid}"))
}

/// The truth right now.
pub fn truth(board: &Board, pid: u32) -> Phase {
    board.phase(agent(board, pid))
}

/// What the chip shows.
pub fn shown(board: &Board, pid: u32) -> Phase {
    agent(board, pid).shown.phase
}

pub fn open_tools(board: &Board, pid: u32) -> Vec<&str> {
    match &agent(board, pid).activity {
        Activity::Turn { tools, .. } => tools.keys().map(|k| k.as_str()).collect(),
        _ => Vec::new(),
    }
}

pub fn gate_of<'b>(board: &'b Board, pid: u32, call: &str) -> Option<&'b Gate> {
    match &agent(board, pid).activity {
        Activity::Turn { tools, .. } => tools.get(call).map(|t| &t.gate),
        _ => None,
    }
}

pub fn publishes(effects: &[Effect]) -> bool {
    effects.contains(&Effect::PublishState)
}

pub fn history(effects: &[Effect]) -> Vec<&HistoryEntry> {
    effects
        .iter()
        .filter_map(|e| match e {
            Effect::AppendHistory(h) => Some(h),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Events
// ---------------------------------------------------------------------------

fn event(
    pid: u32,
    session: &str,
    at: Instant,
    actor: Actor,
    cause: Cause,
    confidence: Confidence,
    kind: EventKind,
) -> Event {
    Event {
        at: Moment {
            local: at,
            claimed: None,
        },
        process: Some(key(pid)),
        session: SessionId::from(session),
        actor,
        cause,
        confidence,
        kind,
    }
}

/// An agent-caused, confirmed event from the agent itself, routed by pid.
pub fn hook(pid: u32, session: &str, at: Instant, kind: EventKind) -> Event {
    event(
        pid,
        session,
        at,
        Actor::Agent,
        Cause::Agent,
        Confidence::Confirmed,
        kind,
    )
}

/// A user-caused event.
pub fn by_user(pid: u32, session: &str, at: Instant, kind: EventKind) -> Event {
    event(
        pid,
        session,
        at,
        Actor::Agent,
        Cause::User,
        Confidence::Confirmed,
        kind,
    )
}

/// An agent-caused event read off a transcript replay.
pub fn inferred(pid: u32, session: &str, at: Instant, kind: EventKind) -> Event {
    event(
        pid,
        session,
        at,
        Actor::Agent,
        Cause::Agent,
        Confidence::Inferred,
        kind,
    )
}

/// An event fired by a subagent of `pid`.
pub fn from_subagent(pid: u32, session: &str, sub: &str, at: Instant, kind: EventKind) -> Event {
    event(
        pid,
        session,
        at,
        Actor::Subagent(SubagentId::from(sub)),
        Cause::Agent,
        Confidence::Confirmed,
        kind,
    )
}

/// The same event, routed by session only (as if the pid were unknown).
pub fn unrouted(mut e: Event) -> Event {
    e.process = None;
    e
}

pub fn turn_started(prompt: &str) -> EventKind {
    EventKind::TurnStarted {
        turn: Some(TurnKey::Prompt(PromptId::from(prompt))),
        origin: Origin::Human,
    }
}

pub fn turn_ended(prompt: Option<&str>, outcome: TurnOutcome) -> EventKind {
    EventKind::TurnEnded {
        turn: prompt.map(|p| TurnKey::Prompt(PromptId::from(p))),
        outcome,
    }
}

pub fn stop() -> EventKind {
    turn_ended(None, TurnOutcome::Completed)
}

pub fn interrupted() -> EventKind {
    turn_ended(None, TurnOutcome::Interrupted { killed: None })
}

pub fn hash(input: &Value) -> InputHash {
    input_hash(input)
}

pub fn dispatched(call: &str, tool: &str, input: &Value) -> EventKind {
    EventKind::ToolDispatched {
        call: ToolUseId::from(call),
        tool: ToolName::from(tool),
        hash: input_hash(input),
    }
}

pub fn settled(call: &str) -> EventKind {
    EventKind::ToolSettled {
        call: ToolUseId::from(call),
        outcome: ToolOutcome::Ok { ms: Some(10) },
    }
}

/// A Claude `PermissionRequest`: no call id, owned by a human.
pub fn asked(tool: &str, input: &Value) -> EventKind {
    EventKind::PermissionAsked {
        call: None,
        tool: ToolName::from(tool),
        hash: Some(input_hash(input)),
        owner: Ownership::Human,
        kind: GateKind::Approval,
    }
}

/// A gate whose owner nobody reported (the Codex hook case).
pub fn asked_unowned(call: &str, tool: &str) -> EventKind {
    EventKind::PermissionAsked {
        call: Some(ToolUseId::from(call)),
        tool: ToolName::from(tool),
        hash: None,
        owner: Ownership::Unknown,
        kind: GateKind::Approval,
    }
}

/// A question tool's `PreToolUse`: the call is the question.
pub fn question(call: &str, tool: &str) -> EventKind {
    EventKind::PermissionAsked {
        call: Some(ToolUseId::from(call)),
        tool: ToolName::from(tool),
        hash: None,
        owner: Ownership::Human,
        kind: GateKind::Question,
    }
}

pub fn limits(status: LimitStatus) -> Snapshot {
    limits_at(status, Instant::now())
}

pub fn limits_at(status: LimitStatus, taken: Instant) -> Snapshot {
    Snapshot {
        taken,
        kind: SnapshotKind::Limits {
            account: reduce::default_account(),
            value: Limits {
                status,
                resets_at: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_788_994_800)),
                window: Some(LimitWindow::FiveHour),
                used_percent: Some(100),
            },
        },
    }
}

pub fn inputs_events(inputs: &[Input]) -> Vec<&Event> {
    inputs
        .iter()
        .filter_map(|i| match i {
            Input::Event(e) => Some(e),
            Input::Snapshot(_) => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Files
// ---------------------------------------------------------------------------

pub fn fixture(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(rel)
}

pub fn fixture_json(rel: &str) -> Value {
    let text = std::fs::read_to_string(fixture(rel)).expect("fixture exists");
    serde_json::from_str(&text).expect("fixture is JSON")
}

/// A fresh, empty directory under the target dir, unique per test name.
/// Not under `/tmp`: that is RAM on the machine this was written for.
pub fn scratch_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// A fake `/proc/<pid>` with the files `proc::read_process` reads.
pub struct FakeProc<'a> {
    pub root: &'a Path,
}

impl FakeProc<'_> {
    pub fn add(&self, pid: u32, comm: &str, ppid: u32, start: u64, argv: &[&str], cwd: &Path) {
        let dir = self.root.join(pid.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let stat = format!(
            "{pid} ({comm}) S {ppid} {pid} {ppid} 34816 {pid} 4194560 100 0 0 0 250 50 0 0 20 0 19 0 {start} 7543691 12590 18446744073709551615 1 1 0 0 0 0 0 16781312 1024 0 0 0 17 5 0 0 0 0 0"
        );
        std::fs::write(dir.join("stat"), stat).unwrap();
        let mut cmdline: Vec<u8> = argv.join("\0").into_bytes();
        cmdline.push(0);
        std::fs::write(dir.join("cmdline"), cmdline).unwrap();
        let _ = std::os::unix::fs::symlink(cwd, dir.join("cwd"));
        let _ = std::os::unix::fs::symlink(
            "/home/u/.local/share/claude/versions/2.1.269",
            dir.join("exe"),
        );
    }

    /// A process whose stat is a zombie.
    pub fn add_zombie(&self, pid: u32, comm: &str, ppid: u32) {
        let dir = self.root.join(pid.to_string());
        std::fs::create_dir_all(&dir).unwrap();
        let stat = format!("{pid} ({comm}) Z {ppid} 0 0 0 0 0 0 0 0 0 0 0 0 0 20 0 1 0 5 0 0");
        std::fs::write(dir.join("stat"), stat).unwrap();
    }
}
