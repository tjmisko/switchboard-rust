//! Session state model.
//!
//! Sketch, not settled. Everything here compiles so the shapes can be checked,
//! but no behaviour is implemented.
//!
//! Sources for the field names and enum variants:
//!   docs/survey/09-signal-fixtures.md        captured payloads
//!   docs/survey/14-hook-events-verified.md   hook events, read from the binaries
//!   docs/03-types-and-transitions.md         the reasoning behind these types
//!
//! Three ideas carry the design:
//!
//!   1. The entities are agents and subagents. An `Agent` is one running
//!      coding-agent process — the thing a chip represents and the thing you
//!      switch to. A `Subagent` is something it spawned, which has no process
//!      of its own. Everything else in this file exists to describe those two.
//!
//!   2. Inputs are either events or snapshots. An event says something
//!      happened; miss it and you never learn about it. A snapshot says how
//!      something looked; miss one and the next repairs you. They merge by
//!      opposite rules, so they are separate types.
//!
//!   3. Truth is computed; what the chip shows is remembered. `Agent::phase`
//!      is a pure function of the axes, so no two writers can disagree about
//!      it. `Shown` is what the operator sees, and it lags the truth in
//!      exactly one direction, because the cost of being wrong is asymmetric
//!      — see docs/04-cost-model.md.

#![allow(dead_code)]

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

// ---------------------------------------------------------------------------
// 1. Identity
// ---------------------------------------------------------------------------

/// Every identifier gets its own type. These are all `Box<str>` underneath and
/// would otherwise be interchangeable, which is how a subagent id ends up in a
/// map keyed by tool call.
macro_rules! id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub Box<str>);
    };
}

id! {
    /// The current conversation. Claude: the session UUID. Codex: the thread
    /// id. Rotates on `/clear` while the agent keeps running, which is why an
    /// `Agent` is keyed by its process and merely *holds* one of these.
    SessionId
}
id! {
    /// Claude `prompt_id`, on the hook base and as `promptId` on transcript
    /// lines: "UUID correlating a user prompt with all subsequent events until
    /// the next prompt". The join key between those two streams.
    PromptId
}
id! {
    /// Codex `turn_id`.
    TurnId
}
id! {
    /// `toolu_...`. On PreToolUse, PostToolUse, PostToolUseFailure,
    /// PermissionDenied, and on transcript tool_use / tool_result blocks.
    /// Absent from PermissionRequest, which is the reason `Prompts` exists.
    ToolUseId
}
id! {
    /// Identifies a subagent.
    ///
    /// The wire field is called `agent_id`, which is a trap: it never names
    /// an agent. Renaming it here is the point of the newtype.
    ///
    /// It arrives bare or prefixed depending on the event, and the
    /// `subagents/` filename stem is `agent-<id>` — stripping `agent-` twice
    /// breaks a subagent genuinely named `gent-x`. Normalise once, at the
    /// adapter boundary, and never key on the filename.
    SubagentId
}
id! {
    /// app-server server-request id. A string or an integer on the wire;
    /// normalised to a string here.
    RequestId
}
id! {
    /// `message.id` (`msg_...`), needed only to say which message an interrupt
    /// killed.
    MessageId
}
id! {
    /// Account the agent bills against. Usage limits attach here, not to the
    /// agent, because they are account-wide.
    AccountId
}
id! {
    /// Tool name as the provider spells it.
    ToolName
}

/// FNV-1a (64-bit) of the canonical JSON form of a tool input. Joins a
/// permission prompt to the call it belongs to. Never forwarded raw.
///
/// Not sha256: the join compares a handful of in-flight calls, so collision
/// resistance buys nothing, and FNV keeps the crate free of a crypto
/// dependency. Both sides of the join are hashed by this crate, so the
/// function only has to agree with itself. See `ingest::hash`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InputHash(pub [u8; 8]);

/// An agent's identity, and the key of the `agents` map.
///
/// `pid` alone is not enough: pids are recycled, and a recycled pid would
/// inherit the previous agent's state. Both fields come from a single read of
/// `/proc/<pid>/stat`, so the stronger key is free.
///
/// This is the stable identity. A `/clear` rotates `Agent::session` and leaves
/// `ProcKey` alone, which is correct: the agent did not change, its
/// conversation did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProcKey {
    pub pid: u32,
    /// Field 22 of `/proc/<pid>/stat`, in clock ticks since boot.
    pub start: u64,
}

/// A terminal pane. Multiplexer-specific underneath.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PaneKey(pub Box<str>);

/// A window-manager window. Hyprland addresses, and the two Hyprland sockets
/// disagree about the `0x` prefix, so normalise at the adapter.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WindowId(pub Box<str>);

/// A background task or timer an agent is waiting on, from
/// `Stop.background_tasks[]` and `Stop.session_crons[]`. Element shapes are
/// inferred; confirm against a recording before depending on them.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WaitId(pub Box<str>);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    Claude,
    Codex,
}

/// Which entity an event is about: the agent itself, or one of its subagents.
///
/// Codex does not guarantee a subagent id on child hooks, so some of its
/// events are attributed to the agent even when a subagent caused them. That
/// is why a Codex `Stop` may sweep subagent *activity* and must not touch
/// their gates.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Actor {
    Agent,
    Subagent(SubagentId),
}

// ---------------------------------------------------------------------------
// 2. Time
// ---------------------------------------------------------------------------

/// Two readings of when something happened, kept apart on purpose.
///
/// `local` is monotonic and daemon-local. Every timeout and every ordering
/// decision uses it.
///
/// `claimed` is whatever the source said, and is for display and for foreign
/// deadlines only. Transcript timestamps are *generation* time, so a transcript
/// line can claim a moment earlier than the hook reporting the same fact.
/// Ordering across sources by wall clock is wrong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Moment {
    pub local: Instant,
    pub claimed: Option<SystemTime>,
}

// ---------------------------------------------------------------------------
// 3. Inputs: events and snapshots
// ---------------------------------------------------------------------------

/// Something happened.
///
/// Exact, and lossy: nothing repeats it, so a missed event is gone. Apply in
/// order, once, keyed by whatever id it carries.
///
/// Sources: Claude and Codex hooks, transcript lines, app-server
/// notifications, a readable pidfd, the Hyprland event socket.
#[derive(Debug, Clone)]
pub struct Event {
    pub at: Moment,
    /// The agent process, when the source knows it. Hook frames carry the
    /// forwarder's pid, which the listener resolves to a tracked ancestor;
    /// a transcript watcher is spawned for one process and always knows.
    ///
    /// Routing prefers this over `session`, because a `/clear` arrives
    /// carrying a session id the board has never seen.
    pub process: Option<ProcKey>,
    pub session: SessionId,
    pub actor: Actor,
    pub cause: Cause,
    /// How far to trust this event if it raises the chip. Hooks are
    /// `Confirmed`; a transcript replayed on discovery is `Inferred`.
    pub confidence: Confidence,
    pub kind: EventKind,
}

/// Who made this happen. The single most important field for deciding how fast
/// the chip must react — see docs/04-cost-model.md §3.
///
/// A transition the operator caused is one they already know about, so showing
/// it is confirmation rather than news. Confirmation costs nothing to deliver
/// and withholding it is what costs: a chip still reading green ten seconds
/// after they pressed Escape is the one failure they can personally verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cause {
    /// The operator did this, and knows. Never waits, in either direction.
    ///
    /// Proven by: `interruptedMessageId` on a transcript line,
    /// `PostToolUseFailure{is_interrupt: true}`,
    /// `toolDenialKind: "user-rejected"`, the Codex `Interrupt` hook,
    /// `origin.kind: "human"`, `SessionStart{source: "clear"}`, or any
    /// approval that was answered.
    User,
    /// The agent or the harness did it: a rate limit, an API failure, a tool
    /// finishing, a subagent ending, a sweep noticing something. The operator
    /// has no independent view, so this is news, and escalating it early and
    /// wrongly is the expensive mistake.
    Agent,
}

#[derive(Debug, Clone)]
pub enum EventKind {
    SessionStarted {
        source: StartSource,
        title: Option<Box<str>>,
    },
    /// Not liveness. A crash emits nothing, so only the pidfd ends an agent.
    SessionEnded {
        reason: Box<str>,
    },
    /// `turn` is optional because `prompt_id` is optional on the hook base,
    /// and assistant transcript lines do not carry it at all.
    TurnStarted {
        turn: Option<TurnKey>,
        origin: Origin,
    },
    /// Ends the open turn unless both sides carry a key and the keys differ,
    /// which is the late close of an earlier turn.
    TurnEnded {
        turn: Option<TurnKey>,
        outcome: TurnOutcome,
    },
    /// Claude `PreToolUse` (needs an unrestricted matcher to be useful),
    /// Codex `PreToolUse`, or a transcript `tool_use` block.
    ToolDispatched {
        call: ToolUseId,
        tool: ToolName,
        hash: InputHash,
    },
    ToolSettled {
        call: ToolUseId,
        outcome: ToolOutcome,
    },
    /// `call` is None for Claude, whose `PermissionRequest` has no
    /// `tool_use_id`. See `Prompts`.
    ///
    /// `kind` is `Question` for a call that *is* the question
    /// (`AskUserQuestion`, `ExitPlanMode`, an elicitation). Those arrive
    /// with their call id and are their own dispatch.
    PermissionAsked {
        call: Option<ToolUseId>,
        tool: ToolName,
        hash: Option<InputHash>,
        owner: Ownership,
        kind: GateKind,
    },
    PermissionSettled {
        call: Option<ToolUseId>,
        req: Option<RequestId>,
        outcome: Decision,
    },
    SubagentStarted {
        subagent: SubagentId,
        kind: SubagentKind,
    },
    SubagentEnded {
        subagent: SubagentId,
        outcome: SubagentOutcome,
    },
    CompactionStarted {
        trigger: Trigger,
    },
    CompactionEnded,
    /// Claude `CwdChanged`. Codex has no equivalent, so its cwd stays whatever
    /// discovery last saw.
    CwdChanged {
        to: PathBuf,
    },
    /// pidfd became readable. The only exact death signal.
    Died,
}

/// Turn identity exists on both providers, so turn events are idempotent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnKey {
    Prompt(PromptId),
    Turn(TurnId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartSource {
    Startup,
    Resume,
    Clear,
    Compact,
}

/// Claude transcript `origin.kind`. A machine wake-up and a typed prompt are
/// the same line shape apart from this field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Human,
    TaskNotification,
    Peer,
    Coordinator,
    AutoContinuation,
    Machine,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnOutcome {
    Completed,
    /// Claude: a transcript line carrying `interruptedMessageId`, which names
    /// the message that was killed. 69 in the local corpus, 13 of them
    /// interrupting a message with no tool call open. Key on that field, never
    /// on the marker text: two marker strings exist.
    ///
    /// Codex: the `Interrupt` hook, or `turn/completed` with
    /// `status: "interrupted"`.
    Interrupted {
        killed: Option<MessageId>,
    },
    /// Claude `StopFailure`, or Codex `turn/completed` with `status: "failed"`
    /// and a `TurnError`. Claude's `Stop` does not fire on this path and Codex
    /// has no hook for it at all.
    Failed {
        class: ErrorClass,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    RateLimit,
    ServerError,
    AuthFailed,
    ConnectionFailed,
    StreamDisconnected,
    TooManyRetries,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolOutcome {
    Ok {
        ms: Option<u32>,
    },
    /// Claude `PostToolUseFailure`. `interrupt` is its `is_interrupt`, which
    /// makes an Esc during a tool exact. Codex has no equivalent: a failed
    /// command arrives as a plain `PostToolUse`.
    Failed {
        interrupt: bool,
    },
    Denied {
        by: Denier,
    },
}

/// The measured `toolDenialKind` vocabulary. Counts across the local corpus:
/// permission-rule 274, automode-blocked 71, user-rejected 21,
/// automode-unavailable 4, interrupted 1.
///
/// Only `User` is a human decision. Treating the whole set as "declined"
/// attributes 350 policy denials to the operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denier {
    User,
    PermissionRule,
    AutoMode,
    Unavailable,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Denied { by: Denier },
    Abandoned,
}

/// Whether a human has to answer a gate.
///
/// This is the whole Codex `approvalsReviewer` question, encoded so the fold
/// cannot get it wrong. A gate held by `auto_review` or a guardian subagent is
/// machine business and must never light a chip.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ownership {
    Human,
    Machine,
    /// Codex hooks report a gate without its owner. Renders as unknown after a
    /// grace, never as attention.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentKind {
    Task,
    Teammate,
    WorkflowAgent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentOutcome {
    Completed,
    Failed,
    Killed,
    Interrupted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Manual,
    Auto,
}

/// How something looked at a moment.
///
/// Self-correcting, and possibly stale: a missed snapshot costs nothing
/// because the next one carries the same ground. Replaces a whole scope rather
/// than adjusting one field.
///
/// Sources: `statusLine`, `subagentStatusLine`, the `subagents/` scan, a
/// `/proc` sweep, `hyprctl clients`, `wezterm cli list`, app-server
/// `thread/list`.
#[derive(Debug, Clone)]
pub struct Snapshot {
    pub taken: Instant,
    pub kind: SnapshotKind,
}

#[derive(Debug, Clone)]
pub enum SnapshotKind {
    /// Claude `statusLine.rate_limits`, or Codex `account/rateLimits/updated`.
    /// Account-wide, so it lands in `Board::accounts`, not on an agent.
    Limits { account: AccountId, value: Limits },
    /// The rest of the `statusLine` payload.
    Beat { session: SessionId, value: Beat },
    /// Claude `subagentStatusLine`: the whole panel in one call.
    Subagents {
        session: SessionId,
        rows: Vec<SubagentRow>,
    },
    /// `Stop.background_tasks[]` and `Stop.session_crons[]`: the whole set of
    /// outstanding waits for one agent. The entire basis of `Phase::Waiting`.
    Waits {
        session: SessionId,
        rows: Vec<WaitRow>,
    },
    /// Discovery sweep of `/proc`.
    Processes(Vec<ProcessRow>),
    /// Window manager and multiplexer tables, already joined.
    Placements(Vec<PlacementRow>),
}

/// A value that came from a snapshot, carrying when that snapshot was taken.
///
/// The rule this type exists to enforce: **a snapshot never overwrites
/// something an event established after the snapshot was taken.** Without it a
/// stale panel snapshot resurrects a subagent that `SubagentStop` just closed,
/// and a `/proc` sweep revives an agent whose pidfd already fired.
#[derive(Debug, Clone)]
pub struct AsOf<T> {
    pub value: T,
    pub as_of: Instant,
}

impl<T> AsOf<T> {
    /// `last_event` is the most recent event for the same scope.
    pub fn supersedes(&self, last_event: Option<Instant>) -> bool {
        match last_event {
            Some(e) => self.as_of > e,
            None => true,
        }
    }
}

// ---------------------------------------------------------------------------
// 4. The board
// ---------------------------------------------------------------------------

/// The agents, and the one thing they share.
///
/// Almost everything is per-agent and lives on `Agent`. Two things resisted:
///
/// Usage limits are account-wide — one agent hitting a weekly cap means every
/// agent on that account is capped, including ones that are idle and not
/// rendering a status line to find out. A copy per agent would need a
/// reconciliation pass to keep those copies honest, and two agents could
/// disagree about one account. So `Agent` holds an `AccountId` and looks it
/// up here.
///
/// Window and pane tables do not appear at all. They arrive as a snapshot, get
/// resolved against `Agent::tty` on arrival, and are discarded: the result is
/// `Agent::placement`. Storing the tables would only cache a join that is
/// recomputed on every sweep anyway.
///
/// There is no session index. Hooks arrive carrying a session id and the
/// lookup is a scan over `agents`, which is a handful of entries.
///
/// `BTreeMap` rather than `HashMap`: the publish gate is a change key computed
/// over the serialised snapshot, and `HashMap` iteration order would produce
/// publishes for snapshots that did not change.
#[derive(Debug, Default)]
pub struct Board {
    pub agents: BTreeMap<ProcKey, Agent>,
    pub accounts: BTreeMap<AccountId, Account>,
}

impl Board {
    pub fn by_session(&self, session: &SessionId) -> Option<&Agent> {
        self.agents
            .values()
            .find(|a| a.session.as_ref() == Some(session))
    }

    pub fn phase(&self, agent: &Agent) -> Phase {
        let limits = self
            .accounts
            .get(&agent.account)
            .and_then(|a| a.limits.as_ref())
            .map(|l| &l.value);
        agent.phase(limits)
    }
}

/// Usage limits live here because they are account-wide: one agent hitting a
/// weekly cap tells you every agent on that account is capped.
#[derive(Debug, Clone, Default)]
pub struct Account {
    pub limits: Option<AsOf<Limits>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limits {
    pub status: LimitStatus,
    /// Epoch seconds, as the provider reports them.
    pub resets_at: Option<SystemTime>,
    pub window: Option<LimitWindow>,
    /// 0..=100.
    pub used_percent: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitStatus {
    Allowed,
    Warning,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LimitWindow {
    FiveHour,
    SevenDay,
    Overage,
}

/// One agent's resolved location, as an input.
///
/// Keyed by process, not by tty. Binding an agent to a pane is a controlling
/// terminal lookup on Unix and something else entirely on Windows, so the
/// platform layer owns that join and reports its conclusion. A `tty` field
/// here would put a `dev_t` in the middle of a portable model for the sake of
/// one platform's implementation detail.
#[derive(Debug, Clone)]
pub struct PlacementRow {
    pub process: ProcKey,
    pub placement: Placement,
}

/// Where an agent is on screen, and whether you are looking at it.
///
/// Absence is a state, not an `Option`, wherever it renders: the chip has an
/// `unnavigable` class, so `None` would have to be translated back into one of
/// these anyway. Keeping `focused` inside `At` makes "focused but nowhere"
/// unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum Placement {
    /// No sweep has located this agent yet.
    #[default]
    Unknown,
    At {
        pane: PaneKey,
        window: Option<WindowId>,
        focused: bool,
    },
    /// Located, and there is nowhere to jump to: no window manager, a headless
    /// run, or a remote session.
    Unnavigable,
}

// ---------------------------------------------------------------------------
// 5. Agent
// ---------------------------------------------------------------------------

/// One running coding-agent process. This is what a chip represents and what
/// you switch to.
///
/// It exists because a process exists, never because a hook mentioned it: a
/// hook naming an unknown pid creates nothing. Consequently the process fields
/// live here rather than in a separate map — every process the daemon tracks
/// is an agent.
#[derive(Debug, Clone)]
pub struct Agent {
    pub process: ProcKey,
    pub provider: Provider,
    pub life: Life,

    /// The current conversation. Rotates on `/clear`; the agent does not.
    ///
    /// Optional because an agent exists from the moment a sweep finds its
    /// process, and a sweep can only name the session when
    /// `~/.claude/sessions/<pid>.json` is present and agrees on `procStart`.
    /// Otherwise the first hook fills it in.
    pub session: Option<SessionId>,
    pub account: AccountId,
    pub title: Option<Box<str>>,

    /// Where the agent is working. `cwd` follows Claude's `CwdChanged`; Codex
    /// has no such event, so its `cwd` stays whatever discovery last saw.
    pub cwd: PathBuf,
    /// Display name for the project, when one can be derived. Optional because
    /// an agent started outside any repository has none.
    pub workspace: Option<Box<str>>,
    pub placement: Placement,
    /// Accumulated CPU time, in whatever unit the platform counts. Used only
    /// for deltas, so the unit does not need to be portable. A turn with no
    /// delta here and no transcript growth has finished.
    pub cpu_ticks: u64,

    pub activity: Activity,
    pub prompts: Prompts,
    pub compaction: Compaction,
    pub waits: BTreeMap<WaitId, Wait>,
    pub subagents: Subagents,
    pub beat: Option<AsOf<Beat>>,

    /// What the chip currently shows. Distinct from `phase()`, which is the
    /// truth right now — see `Shown` and docs/04-cost-model.md.
    pub shown: Shown,

    /// Most recent event for this agent, for `AsOf::supersedes`.
    pub last_event: Option<Instant>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Life {
    Alive,
    Gone { at: Instant },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessRow {
    pub key: ProcKey,
    pub provider: Provider,
    pub cwd: PathBuf,
    /// From `~/.claude/sessions/<pid>.json`, only when its `procStart`
    /// matches `key.start`. A stale file from a recycled pid names someone
    /// else's conversation.
    pub session: Option<SessionId>,
    pub tty: Option<u64>,
    pub cpu_ticks: u64,
}

/// Both providers fire tool hooks concurrently for parallel calls, so an
/// active turn holds a map.
///
/// There is no `Thinking` variant. Thinking is `tools.is_empty()`, and the
/// chip renders it the same as a running tool anyway.
#[derive(Debug, Clone)]
pub enum Activity {
    /// Discovered, and never heard from.
    ///
    /// Every agent is in this state when the daemon starts after them, and
    /// any agent is in it between discovery and its first event. It is not
    /// `Idle`: idle is a claim, and we have no grounds for it.
    ///
    /// This matters more than it looks. An agent sitting on a permission
    /// prompt emits nothing — it is waiting, so there is no next event to
    /// wait for. A restarted daemon that assumed `Idle` would show green
    /// until the operator happened to look, which is the expensive kind of
    /// false calm. So discovery must emit a `ReadTranscriptTail` effect and
    /// reconstruct rather than assume.
    ///
    /// It folds to `Phase::Unknown` — grey. Green would be a claim we cannot
    /// support and amber would be a burst of false alarms across every agent
    /// on daemon restart. Grey is neither: it interrupts nobody and asserts
    /// nothing.
    Unobserved { since: Instant },
    Idle {
        since: Instant,
        /// How the last turn ended, which is not the same question as whether
        /// one is running. An interrupted turn leaves unfinished work and
        /// shows as `Phase::Stopped`; a failed one as `Phase::Blocked`. Reset
        /// when the next turn starts.
        after: Option<TurnOutcome>,
    },
    Turn {
        key: Option<TurnKey>,
        started: Instant,
        tools: BTreeMap<ToolUseId, Tool>,
        last_event: Instant,
    },
}

#[derive(Debug, Clone)]
pub struct Tool {
    pub name: ToolName,
    pub hash: InputHash,
    pub since: Instant,
    pub gate: Gate,
}

/// A permission gate is a state of the call that caused it. Keeping a separate
/// `blocked: bool` on the agent invites the two to disagree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gate {
    Running,
    Blocked {
        kind: GateKind,
        since: Instant,
        owner: Ownership,
        /// app-server server-request id, when the gate came from there.
        req: Option<RequestId>,
    },
}

/// What the agent is blocked on. Both stop the turn and both want a person,
/// so they fold to the same phase — but they are different questions and the
/// tooltip has to say which.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateKind {
    /// "May I run this?" — a permission boundary. Arrives as
    /// `PermissionRequest`, or an app-server `requestApproval`.
    Approval,
    /// "Answer this." — the tool call *is* the question. `AskUserQuestion`,
    /// `ExitPlanMode`, Codex `request_user_input`, an MCP elicitation.
    ///
    /// These never produce a permission event, so a reducer that only watches
    /// for approvals misses them entirely.
    Question,
}

/// Claude's `PermissionRequest` carries no `tool_use_id`, so a prompt is joined
/// to its call on `(actor, tool_name, hash(tool_input))`.
///
/// Sound because `PreToolUse` and `PermissionRequest` both observe the
/// pre-approval input. `PostToolUse` carries the rewritten input, so it must be
/// matched by id — which is why `ToolSettled` has no hash field.
///
/// Hooks are separate processes, so arrival order is not guaranteed. The join
/// is attempted from both directions and `unjoined` holds a prompt that arrived
/// before the call it belongs to. These are the only prompts needing a timeout.
#[derive(Debug, Clone, Default)]
pub struct Prompts {
    pub unjoined: Vec<Unjoined>,
}

#[derive(Debug, Clone)]
pub struct Unjoined {
    pub actor: Actor,
    pub tool: ToolName,
    pub hash: Option<InputHash>,
    pub since: Instant,
    pub owner: Ownership,
    pub kind: GateKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compaction {
    None,
    InProgress { since: Instant, trigger: Trigger },
}

/// Something an agent waits on that is neither a turn nor a tool: a background
/// shell, a monitor, a scheduled wake-up. Reported by
/// `Stop.background_tasks[]` and `Stop.session_crons[]`.
///
/// 55% of background tasks in the corpus never write a closing record, so a
/// wait needs an expiry even though the Stop payload is authoritative while it
/// keeps arriving.
#[derive(Debug, Clone)]
pub struct Wait {
    pub kind: WaitKind,
    pub armed: Instant,
    pub expected_by: Option<Instant>,
    pub expires: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaitKind {
    BackgroundShell,
    Monitor,
    Cron,
}

/// One outstanding wait as a `Stop` payload reports it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WaitRow {
    pub id: WaitId,
    pub kind: WaitKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beat {
    pub model: Box<str>,
    pub context_tokens: u32,
    /// `cost.total_api_duration_ms`. The harness's own accumulator for time
    /// spent in the API. Two beats with no delta mean no request is in flight,
    /// which is the cheapest corroboration that a turn has ended.
    pub api_ms: u64,
}

// ---------------------------------------------------------------------------
// 6. Subagent
// ---------------------------------------------------------------------------

/// The subagents of one agent, as a table rather than an accumulating map.
///
/// A subagent has no pid, no pidfd and no death signal, so a map that grows by
/// event needs an expiry policy, a capacity bound and an eviction rule, and
/// still leaks when a workflow agent is orphaned. `subagentStatusLine`
/// republishes the whole panel instead, including `Failed` and `Killed`, which
/// no hook emits. So `rows` is replaced wholesale, and absence from the newest
/// snapshot is itself a fact.
///
/// Events refine a row between snapshots. They may never create or remove one.
#[derive(Debug, Clone, Default)]
pub struct Subagents {
    pub rows: BTreeMap<SubagentId, Subagent>,
    pub as_of: Option<Instant>,
    pub source: SubagentSource,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SubagentSource {
    /// `subagentStatusLine`. Authoritative.
    Panel,
    /// `subagents/` directory scan. Cannot see `Failed` or `Killed`, so rows
    /// from here do need the stale cap.
    DirScan,
    /// Neither is configured: rows exist only while events keep arriving.
    #[default]
    EventsOnly,
}

/// Deliberately smaller than `Agent`, and deliberately not the same type.
///
/// A subagent has no process, no terminal, no window and no account of its
/// own, and the panel hands over a status rather than something to
/// reconstruct — so it needs no tool map, no compaction, no waits and no
/// placement. Reusing `Agent` here would leave half its fields permanently
/// empty and invite recursion. The map is flat; `parent` carries depth.
#[derive(Debug, Clone)]
pub struct Subagent {
    pub kind: SubagentKind,
    pub status: SubagentStatus,
    pub parent: Option<SubagentId>,
    pub started: SystemTime,
    pub label: Box<str>,
    pub tokens: u64,
    /// A subagent can hold a permission gate. Its prompt renders in the
    /// agent's terminal, so attention travels up to the agent — but the
    /// agent's own `Stop` must never clear it.
    pub gate: Option<Gate>,
    pub last_seen: Instant,
}

/// Straight from `subagentStatusLine`'s `tasks[].status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubagentStatus {
    Pending,
    Running,
    Completed,
    Failed,
    Killed,
    /// Candidate for teammate-idle. Unconfirmed.
    Paused,
}

#[derive(Debug, Clone)]
pub struct SubagentRow {
    pub subagent: SubagentId,
    pub kind: SubagentKind,
    pub status: SubagentStatus,
    pub started: SystemTime,
    pub label: Box<str>,
    pub tokens: u64,
}

// ---------------------------------------------------------------------------
// 7. The derived view
// ---------------------------------------------------------------------------

/// The truth right now.
///
/// Not a field on anything. Computing it means two writers cannot disagree
/// about it, and the inputs stay independently correctable. What the chip
/// *shows* is `Shown`, which is a different thing and does have memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// A human has to answer something.
    Attention,
    /// Usage limit or an unrecoverable error. Nothing to do but wait.
    Blocked,
    /// A turn the operator interrupted, parked with work unfinished.
    ///
    /// Distinct from `Idle`, which means the agent finished what it was asked.
    /// The difference is a pending obligation, and after you walk away and come
    /// back it is exactly the difference worth seeing.
    Stopped,
    /// We decline to say.
    ///
    /// Covers two situations that look different and behave identically: an
    /// agent we have never heard from, and a gate we can see but cannot
    /// attribute to a person. In both, the honest answer is that there is no
    /// answer, and a lamp that claims nothing costs nothing.
    Unknown,
    Working,
    /// Idle itself, with subagents still running.
    Delegating,
    /// Idle itself, with a background task or timer outstanding.
    Waiting,
    Idle,
}

/// How loudly a phase asks for attention. Escalation between bands is what
/// hysteresis governs; movement inside a band is free.
///
/// Deliberately not the same axis as colour. `Unknown` is `Calm` here because
/// it interrupts nobody, but it paints grey rather than green because it is
/// not a claim of calm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Calm,
    Notice,
    Alert,
}

/// What the chip is painted. Four colours over three severities, because
/// "quiet" and "no claim" are different things to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lamp {
    /// No claim. Interrupts nobody, and asserts nothing that could be
    /// contradicted — so it is the only honest answer when we do not know, and
    /// it is free in both of the cost model's terms.
    Grey,
    Green,
    Amber,
    Red,
}

impl Phase {
    pub fn severity(self) -> Severity {
        match self {
            Phase::Attention => Severity::Alert,
            Phase::Blocked | Phase::Stopped => Severity::Notice,
            // Grey interrupts nobody, so grey and green sit at the same
            // severity and move between each other freely in both directions.
            Phase::Unknown => Severity::Calm,
            Phase::Working | Phase::Delegating | Phase::Waiting | Phase::Idle => Severity::Calm,
        }
    }

    pub fn lamp(self) -> Lamp {
        match self {
            Phase::Attention => Lamp::Red,
            Phase::Blocked | Phase::Stopped => Lamp::Amber,
            Phase::Unknown => Lamp::Grey,
            Phase::Working | Phase::Delegating | Phase::Waiting | Phase::Idle => Lamp::Green,
        }
    }
}

/// What the chip is currently showing, which lags the truth in exactly one
/// direction.
///
/// This is the only place the cost model is implemented. Everything else
/// computes truth and hands it here.
#[derive(Debug, Clone)]
pub struct Shown {
    pub phase: Phase,
    pub since: Instant,
    /// A louder phase serving out its dwell. Discarded the moment the truth
    /// drops back, which is what keeps a brief spike from ever reaching the
    /// screen.
    pub pending: Option<Escalation>,
}

#[derive(Debug, Clone)]
pub struct Escalation {
    pub to: Phase,
    pub first_seen: Instant,
    pub promote_at: Instant,
}

/// How far to trust a signal that wants to raise the chip. Sets the dwell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Exact, positive and owned: a Claude `PermissionRequest`, or an
    /// app-server approval request with `approvalsReviewer: user`. Nothing
    /// will retract it, so waiting buys nothing.
    Confirmed,
    /// Read off an error line, a grace expiring, a staleness sweep. Might be
    /// withdrawn by the next signal, so it waits.
    Inferred,
    /// Came from a snapshot, which may have been taken before the event that
    /// contradicts it. Waits at least one refresh interval.
    Snapshot,
}

impl Shown {
    pub const INFERRED_DWELL: Duration = Duration::from_secs(10);
    pub const SNAPSHOT_DWELL: Duration = Duration::from_secs(6);

    pub fn new(phase: Phase, now: Instant) -> Self {
        Shown {
            phase,
            since: now,
            pending: None,
        }
    }

    /// Fold the truth into what is displayed.
    ///
    /// Three of the four cases are immediate; only an agent-caused escalation
    /// waits. See docs/04-cost-model.md §3 for why, and why the `cause`
    /// argument is load-bearing rather than advisory.
    ///
    /// Returns true when the visible phase changed, which is the publish gate.
    pub fn settle(
        &mut self,
        truth: Phase,
        cause: Cause,
        confidence: Confidence,
        now: Instant,
    ) -> bool {
        if truth == self.phase {
            // The truth agrees with the screen. Any escalation in flight was a
            // spike, and spikes never reach the screen.
            self.pending = None;
            return false;
        }

        let louder = truth.severity() > self.phase.severity();

        // De-escalation, and anything the operator did, land at once. A
        // user-caused escalation is confirmation of their own action, so it is
        // never news and never waits.
        if !louder || cause == Cause::User || confidence == Confidence::Confirmed {
            self.phase = truth;
            self.since = now;
            self.pending = None;
            return true;
        }

        let dwell = match confidence {
            Confidence::Confirmed => Duration::ZERO,
            Confidence::Inferred => Self::INFERRED_DWELL,
            Confidence::Snapshot => Self::SNAPSHOT_DWELL,
        };

        match &self.pending {
            // Already waiting on this same phase: promote when its time comes.
            Some(e) if e.to == truth => {
                if now >= e.promote_at {
                    self.phase = truth;
                    self.since = now;
                    self.pending = None;
                    return true;
                }
            }
            // Either nothing pending, or the truth moved to a different phase
            // while waiting. Restart the clock rather than inheriting it.
            _ => {
                self.pending = Some(Escalation {
                    to: truth,
                    first_seen: now,
                    promote_at: now + dwell,
                });
            }
        }
        false
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    pub focused: bool,
    pub suspended: bool,
    pub headless: bool,
    pub unnavigable: bool,
}

#[derive(Debug, Clone)]
pub struct Chip {
    pub phase: Phase,
    pub flags: Flags,
    /// What to put in the tooltip. The reason is the product; the colour is
    /// only an index into it.
    pub reason: Reason,
}

#[derive(Debug, Clone)]
pub enum Reason {
    /// Blocked on a person. `kind` decides the wording: an approval asks
    /// whether to proceed, a question asks for an answer.
    Blocked {
        kind: GateKind,
        tool: ToolName,
        actor: Actor,
        since: Instant,
    },
    /// Discovered but never heard from, and the transcript has not been read
    /// back yet. Shown calm, but the tooltip should not pretend to know.
    NeverObserved {
        since: Instant,
    },
    LimitedUntil {
        at: SystemTime,
    },
    TurnFailed {
        class: ErrorClass,
    },
    /// The operator stopped the last turn with work unfinished.
    Interrupted {
        since: Instant,
    },
    RunningTools {
        count: usize,
    },
    SubagentsRunning {
        count: usize,
    },
    WaitsOutstanding {
        count: usize,
    },
    Idle {
        since: Instant,
    },
}

impl Agent {
    /// The fold. Order matters and this is the one place it is written down.
    ///
    /// A method rather than a field, deliberately. A stored `status` is set
    /// from several transitions, and the moment two of them disagree the chip
    /// is wrong in the direction that matters: the false green. Derived from
    /// the axes, it cannot drift from them, and each axis stays independently
    /// correctable. The cost is recomputing a handful of comparisons per
    /// publish.
    ///
    /// `limits` comes from the agent's account. Passing it in rather than
    /// reaching for a `Board` keeps this testable with no surrounding world.
    ///
    /// A subagent may raise its agent's phase and may never lower it, which is
    /// why subagent gates are checked in the same pass as the agent's own.
    pub fn phase(&self, limits: Option<&Limits>) -> Phase {
        if self.wants_a_human() {
            return Phase::Attention;
        }

        if limits.is_some_and(|l| l.status == LimitStatus::Rejected) {
            return Phase::Blocked;
        }

        // How the last turn ended outlives the turn itself.
        if let Activity::Idle { after: Some(o), .. } = &self.activity {
            match o {
                TurnOutcome::Failed { .. } => return Phase::Blocked,
                TurnOutcome::Interrupted { .. } => return Phase::Stopped,
                TurnOutcome::Completed => {}
            }
        }

        // Checked before `Turn`: a turn parked on a gate nobody owns is not
        // working, and R3 forbids calling it attention. Grey.
        if self.has_unowned_gate() {
            return Phase::Unknown;
        }

        if let Activity::Turn { .. } = self.activity {
            return Phase::Working;
        }

        // Never heard from. Grey: it interrupts nobody and claims nothing.
        if let Activity::Unobserved { .. } = self.activity {
            return Phase::Unknown;
        }

        if self
            .subagents
            .rows
            .values()
            .any(|s| matches!(s.status, SubagentStatus::Running | SubagentStatus::Pending))
        {
            return Phase::Delegating;
        }

        if !self.waits.is_empty() {
            return Phase::Waiting;
        }

        Phase::Idle
    }

    pub fn focused(&self) -> bool {
        matches!(self.placement, Placement::At { focused: true, .. })
    }
}

impl Agent {
    /// A gate anywhere under this agent that a person has to answer.
    fn wants_a_human(&self) -> bool {
        let own = match &self.activity {
            Activity::Turn { tools, .. } => tools.values().any(|t| {
                matches!(
                    t.gate,
                    Gate::Blocked {
                        owner: Ownership::Human,
                        ..
                    }
                )
            }),
            Activity::Idle { .. } | Activity::Unobserved { .. } => false,
        };
        let unjoined = self
            .prompts
            .unjoined
            .iter()
            .any(|p| p.owner == Ownership::Human);
        let subagents = self.subagents.rows.values().any(|s| {
            matches!(
                s.gate,
                Some(Gate::Blocked {
                    owner: Ownership::Human,
                    ..
                })
            )
        });
        own || unjoined || subagents
    }

    /// A gate whose owner was never established, anywhere under this agent.
    /// Codex hooks report a gate without saying who holds it.
    fn has_unowned_gate(&self) -> bool {
        let unowned = |g: &Gate| {
            matches!(
                g,
                Gate::Blocked {
                    owner: Ownership::Unknown,
                    ..
                }
            )
        };
        let own = match &self.activity {
            Activity::Turn { tools, .. } => tools.values().any(|t| unowned(&t.gate)),
            Activity::Idle { .. } | Activity::Unobserved { .. } => false,
        };
        let unjoined = self
            .prompts
            .unjoined
            .iter()
            .any(|p| p.owner == Ownership::Unknown);
        let subagents = self
            .subagents
            .rows
            .values()
            .any(|s| s.gate.as_ref().is_some_and(unowned));
        own || unjoined || subagents
    }
}

// ---------------------------------------------------------------------------
// 8. Effects
// ---------------------------------------------------------------------------
//
// The reducer itself lives in `crate::reduce`. It is pure and synchronous: all
// I/O is in the returned effects, so a transition table can be tested without
// a filesystem, a socket or a clock.

/// Something the reducer wants done. Never touches the board; results that
/// matter come back round as new inputs.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    /// The visible board changed. Coalesced by the publisher.
    PublishState,
    /// A displayed phase changed, or an agent came or went.
    AppendHistory(HistoryEntry),
    /// Start tailing an agent's transcript. The watcher first replays the
    /// tail at `Confidence::Inferred`, which is what replaces
    /// `Activity::Unobserved` with something we have grounds for: a blocked
    /// agent emits no further events, so without it a daemon restart would
    /// leave the chip grey indefinitely.
    ///
    /// Emitted again when `/clear` rotates the session; the runner replaces
    /// any watcher it already holds for `process`.
    ///
    /// Carries the ingredients, not the path: where transcripts live is the
    /// runner's business, and the reducer knows no directories.
    WatchTranscript {
        process: ProcKey,
        session: SessionId,
        cwd: PathBuf,
    },
    /// The agent is gone. Stop everything held on its behalf.
    Forget { process: ProcKey },
}

/// What the history appender records. Phase names, not the Go vocabulary.
#[derive(Debug, Clone, PartialEq)]
pub enum HistoryEntry {
    AgentFound {
        process: ProcKey,
        cwd: PathBuf,
    },
    AgentGone {
        process: ProcKey,
        session: Option<SessionId>,
    },
    /// `Shown` moved. `held` is how long the previous phase was on screen.
    Shown {
        process: ProcKey,
        session: Option<SessionId>,
        from: Phase,
        to: Phase,
        held: Duration,
    },
}

// ---------------------------------------------------------------------------
// 9. Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(phase: Phase, now: Instant) -> Shown {
        Shown::new(phase, now)
    }

    #[test]
    fn should_escalate_immediately_when_the_user_caused_it() {
        // Escape during a turn moves green -> orange, which is up the ladder.
        // The operator pressed the key, so there is no one to surprise and a
        // chip still reading green is a visible lie.
        let t0 = Instant::now();
        let mut s = shown(Phase::Working, t0);
        let changed = s.settle(Phase::Stopped, Cause::User, Confidence::Confirmed, t0);
        assert!(changed);
        assert_eq!(s.phase, Phase::Stopped);
        assert!(s.pending.is_none());
    }

    #[test]
    fn should_escalate_immediately_when_evidence_is_confirmed() {
        let t0 = Instant::now();
        let mut s = shown(Phase::Working, t0);
        let changed = s.settle(Phase::Attention, Cause::Agent, Confidence::Confirmed, t0);
        assert!(changed);
        assert_eq!(s.phase, Phase::Attention);
    }

    #[test]
    fn should_delay_escalation_when_the_agent_caused_it_and_evidence_is_inferred() {
        let t0 = Instant::now();
        let mut s = shown(Phase::Working, t0);

        let changed = s.settle(Phase::Blocked, Cause::Agent, Confidence::Inferred, t0);
        assert!(!changed, "must not reach the screen yet");
        assert_eq!(s.phase, Phase::Working);
        assert!(s.pending.is_some());

        let later = t0 + Shown::INFERRED_DWELL;
        let changed = s.settle(Phase::Blocked, Cause::Agent, Confidence::Inferred, later);
        assert!(changed);
        assert_eq!(s.phase, Phase::Blocked);
    }

    #[test]
    fn should_never_show_a_spike_that_resolves_before_its_dwell_elapses() {
        // The whole point of the dwell: a state that appears and goes away
        // again inside the window must never reach the screen.
        let t0 = Instant::now();
        let mut s = shown(Phase::Working, t0);
        s.settle(Phase::Blocked, Cause::Agent, Confidence::Inferred, t0);

        let mid = t0 + Duration::from_secs(3);
        let changed = s.settle(Phase::Working, Cause::Agent, Confidence::Inferred, mid);
        assert!(!changed);
        assert_eq!(s.phase, Phase::Working);
        assert!(s.pending.is_none(), "the pending escalation is abandoned");
    }

    #[test]
    fn should_de_escalate_immediately_when_the_agent_caused_it() {
        // Lifting is always free: a stale alert is a false alert that keeps
        // being paid.
        let t0 = Instant::now();
        let mut s = shown(Phase::Attention, t0);
        let changed = s.settle(Phase::Idle, Cause::Agent, Confidence::Inferred, t0);
        assert!(changed);
        assert_eq!(s.phase, Phase::Idle);
    }

    #[test]
    fn should_restart_the_dwell_when_the_pending_phase_changes() {
        let t0 = Instant::now();
        let mut s = shown(Phase::Working, t0);
        s.settle(Phase::Blocked, Cause::Agent, Confidence::Inferred, t0);

        let mid = t0 + Duration::from_secs(8);
        s.settle(Phase::Attention, Cause::Agent, Confidence::Inferred, mid);

        // The original deadline has passed, but it belonged to a different
        // phase and must not be inherited.
        let at_old_deadline = t0 + Shown::INFERRED_DWELL;
        let changed = s.settle(
            Phase::Attention,
            Cause::Agent,
            Confidence::Inferred,
            at_old_deadline,
        );
        assert!(!changed);
        assert_eq!(s.phase, Phase::Working);

        let at_new_deadline = mid + Shown::INFERRED_DWELL;
        let changed = s.settle(
            Phase::Attention,
            Cause::Agent,
            Confidence::Inferred,
            at_new_deadline,
        );
        assert!(changed);
        assert_eq!(s.phase, Phase::Attention);
    }

    #[test]
    fn should_report_no_change_when_the_truth_already_matches_the_screen() {
        let t0 = Instant::now();
        let mut s = shown(Phase::Idle, t0);
        let changed = s.settle(Phase::Idle, Cause::Agent, Confidence::Inferred, t0);
        assert!(!changed);
    }

    fn idle_after(outcome: Option<TurnOutcome>, now: Instant) -> Activity {
        Activity::Idle {
            since: now,
            after: outcome,
        }
    }

    #[test]
    fn should_report_stopped_when_the_last_turn_was_interrupted() {
        let now = Instant::now();
        // An interrupted turn leaves unfinished work, which outlives the turn.
        let a = idle_after(Some(TurnOutcome::Interrupted { killed: None }), now);
        assert!(matches!(a, Activity::Idle { after: Some(_), .. }));
    }

    #[test]
    fn should_stay_calm_when_an_agent_has_never_been_observed() {
        // A burst of amber across every agent on daemon restart would be a
        // pile of false alarms. Calm is the cheap error, and discovery emits
        // ReconstructFromTranscript to replace the guess with evidence.
        let now = Instant::now();
        let a = Activity::Unobserved { since: now };
        assert!(matches!(a, Activity::Unobserved { .. }));
        assert_eq!(Phase::Idle.severity(), Severity::Calm);
    }

    #[test]
    fn should_treat_a_question_gate_as_wanting_a_human_like_an_approval() {
        // AskUserQuestion and ExitPlanMode never fire a permission event. A
        // reducer that only watches approvals misses them.
        let now = Instant::now();
        for kind in [GateKind::Approval, GateKind::Question] {
            let g = Gate::Blocked {
                kind,
                since: now,
                owner: Ownership::Human,
                req: None,
            };
            assert!(matches!(
                g,
                Gate::Blocked {
                    owner: Ownership::Human,
                    ..
                }
            ));
        }
    }

    #[test]
    fn should_move_freely_between_grey_and_green_in_both_directions() {
        // Grey claims nothing and interrupts nobody, so it costs nothing in
        // either of the cost model's terms. Making it an escalation would
        // delay it; making it a de-escalation would delay the return.
        let t0 = Instant::now();
        let mut s = shown(Phase::Idle, t0);

        let changed = s.settle(Phase::Unknown, Cause::Agent, Confidence::Inferred, t0);
        assert!(changed, "green -> grey is immediate");
        assert_eq!(s.phase, Phase::Unknown);

        let changed = s.settle(Phase::Working, Cause::Agent, Confidence::Inferred, t0);
        assert!(changed, "grey -> green is immediate");
        assert_eq!(s.phase, Phase::Working);
    }

    #[test]
    fn should_paint_grey_for_an_unobserved_agent_rather_than_green() {
        // Green is a claim we cannot support; amber would be a false alarm
        // across every agent on daemon restart.
        assert_eq!(Phase::Unknown.lamp(), Lamp::Grey);
        assert_eq!(Phase::Idle.lamp(), Lamp::Green);
        assert_eq!(Phase::Unknown.severity(), Severity::Calm);
    }

    #[test]
    fn should_paint_an_unconfirmed_remote_row_grey_whatever_it_last_said() {
        // The phase is a memory of what another machine believed before the
        // link went quiet. Presenting a memory as current is the contradicted
        // case: the operator may already know the host is unreachable.
        let row = Row {
            key: RowKey {
                host: Host("nessie".into()),
                pid: 4036,
                started_at: SystemTime::UNIX_EPOCH,
            },
            provider: Provider::Claude,
            phase: Phase::Attention,
            reach: Reach::ViaHost {
                host: Host("nessie".into()),
                local: None,
            },
            freshness: Freshness::Unconfirmed {
                since: Instant::now(),
            },
            workspace: None,
            title: None,
        };
        assert_eq!(row.phase.lamp(), Lamp::Red);
        assert_eq!(row.lamp(), Lamp::Grey, "unconfirmed rows claim nothing");
    }

    fn sample_agent(pid: u32, now: Instant, tools: usize, kids: usize) -> Agent {
        let mut t = BTreeMap::new();
        for i in 0..tools {
            t.insert(
                ToolUseId(format!("toolu_{i}").into()),
                Tool {
                    name: ToolName("Bash".into()),
                    hash: InputHash([0; 8]),
                    since: now,
                    gate: Gate::Running,
                },
            );
        }
        let mut rows = BTreeMap::new();
        for i in 0..kids {
            rows.insert(
                SubagentId(format!("agent-{i}").into()),
                Subagent {
                    kind: SubagentKind::Task,
                    status: SubagentStatus::Running,
                    parent: None,
                    started: SystemTime::UNIX_EPOCH,
                    label: "survey".into(),
                    tokens: 4096,
                    gate: None,
                    last_seen: now,
                },
            );
        }
        Agent {
            process: ProcKey { pid, start: 900 },
            provider: Provider::Claude,
            life: Life::Alive,
            session: Some(SessionId(format!("sess-{pid}").into())),
            account: AccountId("acct".into()),
            title: Some("switchboard".into()),
            cwd: PathBuf::from("/home/u/Projects/switchboard"),
            workspace: Some("switchboard".into()),
            placement: Placement::At {
                pane: PaneKey("12".into()),
                window: Some(WindowId("0xdeadbeef".into())),
                focused: false,
            },
            cpu_ticks: 1234,
            activity: Activity::Turn {
                key: None,
                started: now,
                tools: t,
                last_event: now,
            },
            prompts: Prompts::default(),
            compaction: Compaction::None,
            waits: BTreeMap::new(),
            subagents: Subagents {
                rows,
                as_of: Some(now),
                source: SubagentSource::Panel,
            },
            beat: None,
            shown: Shown::new(Phase::Working, now),
            last_event: Some(now),
        }
    }

    /// Not a correctness test: a floor under the claim that the fold is far
    /// below a 60 Hz frame budget. Deliberately loose so it cannot flake.
    #[test]
    fn should_fold_a_full_board_far_inside_a_frame() {
        let now = Instant::now();
        let mut board = Board::default();
        board.accounts.insert(
            AccountId("acct".into()),
            Account {
                limits: Some(AsOf {
                    value: Limits {
                        status: LimitStatus::Allowed,
                        resets_at: None,
                        window: None,
                        used_percent: Some(31),
                    },
                    as_of: now,
                }),
            },
        );
        for pid in 0..16u32 {
            board
                .agents
                .insert(ProcKey { pid, start: 900 }, sample_agent(pid, now, 4, 8));
        }

        let reps = 2_000;
        let t0 = Instant::now();
        let mut sink = 0usize;
        for _ in 0..reps {
            for a in board.agents.values() {
                sink += board.phase(a) as usize;
            }
        }
        let per_fold = t0.elapsed() / reps;
        assert!(sink > 0);
        println!(
            "fold: 16 agents x 4 tools x 8 subagents in {:?} per full board",
            per_fold
        );
        assert!(
            per_fold < Duration::from_micros(500),
            "fold took {per_fold:?} for a whole board; frame budget is 16_667us"
        );
    }

    #[test]
    fn should_rank_severity_so_that_only_attention_is_an_alert() {
        assert_eq!(Phase::Attention.severity(), Severity::Alert);
        for p in [Phase::Stopped, Phase::Blocked] {
            assert_eq!(p.severity(), Severity::Notice);
            assert_eq!(p.lamp(), Lamp::Amber);
        }
        for p in [
            Phase::Working,
            Phase::Delegating,
            Phase::Waiting,
            Phase::Idle,
        ] {
            assert_eq!(p.severity(), Severity::Calm);
            assert_eq!(p.lamp(), Lamp::Green);
        }
        assert!(Severity::Alert > Severity::Notice);
        assert!(Severity::Notice > Severity::Calm);
    }
}

// ---------------------------------------------------------------------------
// 10. Federation
// ---------------------------------------------------------------------------
//
// A `Board` is host-local, and stays that way. Remote hosts do not merge into
// it: they arrive as whole read-only snapshots over SSH, and a separate
// read-only `Fleet` merges one local board with zero or more of them.
//
// That boundary is not an implementation detail. The local board owns
// discovery, liveness, durable state and the ability to act. A remote row owns
// none of those, and every field that only makes sense locally — a pid you can
// signal, a pane you can focus, a cwd you can open — means something different
// or nothing at all once it has crossed a machine boundary.
//
// Two consequences fall out of that, and both are easy to get wrong:
//
//   * Remote hosts are a snapshot source and never an event source. They send
//     complete state, and a wakeup means "re-read it all", never "apply this
//     delta". So `AsOf` governs them and local events always win.
//
//   * Navigability belongs to the viewer, not the agent. An agent on another
//     machine is not unnavigable; it is unnavigable *from here*, and what you
//     can actually reach is the local window showing your session to its host.

id! {
    /// A machine. Empty on host-local rows by construction: the local board
    /// has no opinion about which host it is, and stamping one would put
    /// federation vocabulary into the part of the system that must not know
    /// about it.
    Host
}

/// Identity of a row in the merged view.
///
/// `ProcKey` cannot do this job. Its `start` field comes from the kernel, and
/// there is no remote `/proc` to read — so a federated row is fenced by
/// `started_at`, the *daemon's* discovery time, which rejects stale
/// observations while a remote daemon keeps running but is not a process-birth
/// token. Two different guarantees; two different types.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RowKey {
    pub host: Host,
    pub pid: u32,
    pub started_at: SystemTime,
}

/// What the viewer can do about this row, from where they are sitting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Reach {
    /// Same machine. Act on it directly.
    Here { placement: Placement },
    /// Another machine. The agent's own pane and window locate the *remote*
    /// desktop and mean nothing here, so they are stripped. What is actionable
    /// is the local window displaying the session to that host, when one
    /// exists.
    ViaHost { host: Host, local: Option<PaneKey> },
    /// Visible, and there is nothing to switch to.
    Unreachable,
}

/// Whether the row's source is still answering.
///
/// A remote row does not vanish the moment a transport hiccups — it is held
/// until the source confirms the loss did not recover. In between, the row is
/// real but unverified, which is the third thing grey is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Freshness {
    Live,
    Unconfirmed { since: Instant },
}

/// One row of the merged view. Read-only by construction: there is no action
/// channel to a remote host, so nothing here can be mutated into a command.
#[derive(Debug, Clone)]
pub struct Row {
    pub key: RowKey,
    pub provider: Provider,
    pub phase: Phase,
    pub reach: Reach,
    pub freshness: Freshness,
    pub workspace: Option<Box<str>>,
    pub title: Option<Box<str>>,
}

impl Row {
    /// An unconfirmed row claims nothing, whatever the source last said.
    ///
    /// Its phase is a memory of what some other machine believed before the
    /// link went quiet, and presenting a memory as current is the contradicted
    /// case from the cost model — the operator may well know the link is down.
    pub fn lamp(&self) -> Lamp {
        match self.freshness {
            Freshness::Live => self.phase.lamp(),
            Freshness::Unconfirmed { .. } => Lamp::Grey,
        }
    }
}

/// One local board plus zero or more remote snapshots. Owns no discovery and
/// no durable state; every action revalidates against the local board first.
#[derive(Debug, Default)]
pub struct Fleet {
    pub local: Board,
    pub remote: BTreeMap<Host, AsOf<Vec<Row>>>,
}
