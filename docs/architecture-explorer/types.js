// Current golden declarations. Code excerpts omit documentation comments only.
window.SB_TYPES = [
  {
    "id": "frame",
    "name": "Request · HookFrame",
    "group": "Boundary",
    "summary": "A socket line is decoded into a request before becoming a model input. HookFrame keeps the scrubbed JSON payload for the Claude adapter; there is no provider field on this wire format yet.",
    "code": "pub enum Request {\n    Hook(HookFrame),\n    Statusline(StatuslineFrame),\n    List,\n    Subscribe,\n}\n\npub struct HookFrame {\n    pub event: Box<str>,\n    pub pid: Option<u32>,\n    pub session: SessionId,\n    pub payload: Value,\n}\n\npub struct StatuslineFrame {\n    pub pid: Option<u32>,\n    pub session: SessionId,\n    pub payload: Value,\n}",
    "fields": [
      {
        "name": "Request",
        "type": "enum",
        "meaning": "Hook and Statusline submit observations; List and Subscribe read the published View."
      },
      {
        "name": "event",
        "type": "Box<str>",
        "meaning": "Provider hook name, such as PermissionRequest."
      },
      {
        "name": "pid",
        "type": "Option<u32>",
        "meaning": "Process id supplied by the forwarder; RPC resolves it to a tracked ancestor."
      },
      {
        "name": "session",
        "type": "SessionId",
        "meaning": "Conversation identity extracted during frame decoding."
      },
      {
        "name": "payload",
        "type": "serde_json::Value",
        "meaning": "Event-specific JSON. The forwarder replaces tool_input with its hash."
      }
    ],
    "usedBy": [
      "ingest::frame::decode_request",
      "daemon::rpc::handle",
      "ingest::claude::hook"
    ],
    "source": {
      "path": "src/ingest/frame.rs",
      "line": 26
    },
    "status": "implemented"
  },
  {
    "id": "input",
    "name": "Input · Ctx",
    "group": "Input",
    "summary": "All observation producers meet at this two-variant enum. The bounded mpsc channel carries Input into the one task that owns Board. Timeouts call tick directly; there is no Tick or Replay variant in this golden.",
    "code": "pub enum Input {\n    Event(Event),\n    Snapshot(Snapshot),\n}\n\npub struct Ctx {\n    pub now: Instant,\n    pub process: Option<ProcKey>,\n}",
    "fields": [
      {
        "name": "Event",
        "type": "Event",
        "meaning": "An observed edge: a turn began, a tool settled, or a process died."
      },
      {
        "name": "Snapshot",
        "type": "Snapshot",
        "meaning": "An observation of a whole named scope, interpreted using scope-specific rules."
      },
      {
        "name": "Ctx.now",
        "type": "Instant",
        "meaning": "Receipt time supplied to the pure adapter."
      },
      {
        "name": "Ctx.process",
        "type": "Option<ProcKey>",
        "meaning": "Resolved process when the driver can identify it."
      }
    ],
    "usedBy": [
      "ingest::claude",
      "daemon::scanner",
      "daemon::watcher",
      "daemon::core",
      "reduce::apply"
    ],
    "source": {
      "path": "src/ingest/mod.rs",
      "line": 23
    },
    "status": "implemented"
  },
  {
    "id": "identity",
    "name": "ProcKey · Actor · Provider",
    "group": "Input",
    "summary": "A process survives conversation changes, so pid plus kernel start time identifies an agent and SessionId identifies its current conversation. Actor routes work within that agent. Provider remains a closed enum in current code; an open provider identifier is a redesign proposal.",
    "code": "pub struct ProcKey {\n    pub pid: u32,\n    pub start: u64,\n}\n\npub enum Actor {\n    Agent,\n    Subagent(SubagentId),\n}\n\npub enum Provider {\n    Claude,\n    Codex,\n}",
    "fields": [
      {
        "name": "pid",
        "type": "u32",
        "meaning": "Operating-system process id."
      },
      {
        "name": "start",
        "type": "u64",
        "meaning": "Linux /proc start ticks distinguish reused pids."
      },
      {
        "name": "Actor::Agent",
        "type": "variant",
        "meaning": "Work performed by the tracked main agent."
      },
      {
        "name": "Actor::Subagent",
        "type": "SubagentId",
        "meaning": "Work attributed to one child row."
      },
      {
        "name": "Provider",
        "type": "Claude | Codex",
        "meaning": "Both variants exist; only Claude has a wired ingestion path."
      }
    ],
    "usedBy": [
      "Board.agents",
      "Event.process",
      "ProcessRow.key",
      "Event.actor",
      "Agent.provider"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 124
    },
    "status": "implemented"
  },
  {
    "id": "event",
    "name": "Event · Moment",
    "group": "Input",
    "summary": "An event carries routing, observation time, cause, confidence, and a provider-neutral fact. The reducer prefers a tracked process key, then searches by session. Intended duplicate handling is by domain identifiers, not a global event-id log.",
    "code": "pub struct Event {\n    pub at: Moment,\n    pub process: Option<ProcKey>,\n    pub session: SessionId,\n    pub actor: Actor,\n    pub cause: Cause,\n    pub confidence: Confidence,\n    pub kind: EventKind,\n}\n\npub struct Moment {\n    pub local: Instant,\n    pub claimed: Option<SystemTime>,\n}\n\npub enum Cause {\n    User,\n    Agent,\n}\n\npub enum Confidence {\n    Confirmed,\n    Inferred,\n    Snapshot,\n}",
    "fields": [
      {
        "name": "at",
        "type": "Moment",
        "meaning": "local is monotonic receipt time; claimed is optional source wall time."
      },
      {
        "name": "process",
        "type": "Option<ProcKey>",
        "meaning": "Preferred routing identity, when tracked."
      },
      {
        "name": "session",
        "type": "SessionId",
        "meaning": "Conversation identity; rejects late events after a session rotation."
      },
      {
        "name": "actor",
        "type": "Actor",
        "meaning": "Main agent or a particular subagent."
      },
      {
        "name": "cause",
        "type": "Cause",
        "meaning": "User-caused changes can update the display immediately."
      },
      {
        "name": "confidence",
        "type": "Confidence",
        "meaning": "Confirmed, Inferred, or Snapshot determines escalation dwell."
      },
      {
        "name": "kind",
        "type": "EventKind",
        "meaning": "The fact that the reducer applies."
      }
    ],
    "usedBy": [
      "ingest::claude::hook",
      "ingest::claude::line",
      "reduce::route",
      "reduce::apply_event"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 194
    },
    "status": "implemented"
  },
  {
    "id": "event-kind",
    "name": "EventKind",
    "group": "Input",
    "summary": "The reducer speaks turns, calls, gates, children, directories, and liveness. Provider hook names are translated into this vocabulary at ingestion. These are the actual variants; proposed ownership-change and unsupported-event variants are absent.",
    "code": "pub enum EventKind {\n    SessionStarted {\n        source: StartSource,\n        title: Option<Box<str>>,\n    },\n    SessionEnded {\n        reason: Box<str>,\n    },\n    TurnStarted {\n        turn: Option<TurnKey>,\n        origin: Origin,\n    },\n    TurnEnded {\n        turn: Option<TurnKey>,\n        outcome: TurnOutcome,\n    },\n    ToolDispatched {\n        call: ToolUseId,\n        tool: ToolName,\n        hash: InputHash,\n    },\n    ToolSettled {\n        call: ToolUseId,\n        outcome: ToolOutcome,\n    },\n    PermissionAsked {\n        call: Option<ToolUseId>,\n        tool: ToolName,\n        hash: Option<InputHash>,\n        owner: Ownership,\n        kind: GateKind,\n    },\n    PermissionSettled {\n        call: Option<ToolUseId>,\n        req: Option<RequestId>,\n        outcome: Decision,\n    },\n    SubagentStarted {\n        subagent: SubagentId,\n        kind: SubagentKind,\n    },\n    SubagentEnded {\n        subagent: SubagentId,\n        outcome: SubagentOutcome,\n    },\n    CompactionStarted {\n        trigger: Trigger,\n    },\n    CompactionEnded,\n    CwdChanged {\n        to: PathBuf,\n    },\n    Died,\n}",
    "fields": [
      {
        "name": "SessionStarted / SessionEnded",
        "type": "variants",
        "meaning": "Conversation lifecycle; SessionEnded is deliberately not proof of process death."
      },
      {
        "name": "TurnStarted / TurnEnded",
        "type": "variants",
        "meaning": "Work starts or ends, optionally fenced by TurnKey."
      },
      {
        "name": "ToolDispatched / ToolSettled",
        "type": "variants",
        "meaning": "Calls keyed by ToolUseId; dispatch carries the pre-approval input hash."
      },
      {
        "name": "PermissionAsked / PermissionSettled",
        "type": "variants",
        "meaning": "Gate creation and release, optionally keyed by call or request."
      },
      {
        "name": "SubagentStarted / SubagentEnded",
        "type": "variants",
        "meaning": "Child lifecycle observations."
      },
      {
        "name": "CompactionStarted / CompactionEnded",
        "type": "variants",
        "meaning": "Independent compaction state."
      },
      {
        "name": "CwdChanged",
        "type": "variant",
        "meaning": "A new working directory."
      },
      {
        "name": "Died",
        "type": "variant",
        "meaning": "Removes a process from Board. The running scanner derives it from sweep differences."
      }
    ],
    "usedBy": [
      "ingest::claude::hook",
      "ingest::claude::line",
      "reduce::event::transition"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 237
    },
    "status": "implemented"
  },
  {
    "id": "snapshot",
    "name": "Snapshot · SnapshotKind",
    "group": "Input",
    "summary": "Snapshots carry a receipt-time anchor and one named scope. Merge rules differ: a child panel or waits list replaces that table; Processes discovers and refreshes rows, while the scanner sends separate Died events for disappearance. Placements and Subagents have reducer support but no running source.",
    "code": "pub struct Snapshot {\n    pub taken: Instant,\n    pub kind: SnapshotKind,\n}\n\npub enum SnapshotKind {\n    Limits { account: AccountId, value: Limits },\n    Beat { session: SessionId, value: Beat },\n    Subagents {\n        session: SessionId,\n        rows: Vec<SubagentRow>,\n    },\n    Waits {\n        session: SessionId,\n        rows: Vec<WaitRow>,\n    },\n    Processes(Vec<ProcessRow>),\n    Placements(Vec<PlacementRow>),\n}",
    "fields": [
      {
        "name": "taken",
        "type": "Instant",
        "meaning": "Local time associated with the observation."
      },
      {
        "name": "Limits",
        "type": "{ account, value }",
        "meaning": "Shared account information can change several agents at once."
      },
      {
        "name": "Beat",
        "type": "{ session, value }",
        "meaning": "Session heartbeat fields such as API duration."
      },
      {
        "name": "Subagents",
        "type": "{ session, rows }",
        "meaning": "Whole child table; stale snapshots can be rejected using last_event."
      },
      {
        "name": "Waits",
        "type": "{ session, rows }",
        "meaning": "Outstanding background tasks, replaced as a set."
      },
      {
        "name": "Processes",
        "type": "Vec<ProcessRow>",
        "meaning": "Discovery/update observations; absence here alone does not remove an agent."
      },
      {
        "name": "Placements",
        "type": "Vec<PlacementRow>",
        "meaning": "Already-resolved window and pane information; reducer-only support today."
      }
    ],
    "usedBy": [
      "daemon::scanner",
      "ingest::claude::statusline",
      "ingest::claude::hook",
      "reduce::apply_snapshot"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 450
    },
    "status": "implemented"
  },
  {
    "id": "as-of",
    "name": "AsOf<T>",
    "group": "Input",
    "summary": "A generic snapshot value plus its observation time. supersedes accepts a snapshot only when its timestamp is strictly newer than a supplied last-event time. It is a helper used by particular scopes, not a global guarantee that every snapshot branch enforces.",
    "code": "pub struct AsOf<T> {\n    pub value: T,\n    pub as_of: Instant,\n}",
    "fields": [
      {
        "name": "value",
        "type": "T",
        "meaning": "The observed value: Limits, Beat, or a remote row collection."
      },
      {
        "name": "as_of",
        "type": "Instant",
        "meaning": "When the driver took the snapshot in local monotonic time."
      }
    ],
    "usedBy": [
      "Account.limits",
      "Agent.beat",
      "reduce::snapshot::apply_snapshot",
      "Fleet.remote"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 486
    },
    "status": "implemented"
  },
  {
    "id": "board",
    "name": "Board · Account",
    "group": "State",
    "summary": "The live local model is two ordered maps. One core task owns this value, so producers never lock or mutate it. Accounts share usage limits across agents. There is no dead-process tombstone set or session index in the current declaration.",
    "code": "pub struct Board {\n    pub agents: BTreeMap<ProcKey, Agent>,\n    pub accounts: BTreeMap<AccountId, Account>,\n}\n\npub struct Account {\n    pub limits: Option<AsOf<Limits>>,\n}",
    "fields": [
      {
        "name": "agents",
        "type": "BTreeMap<ProcKey, Agent>",
        "meaning": "Stable process identities to independent agent machines; ordered iteration gives stable views."
      },
      {
        "name": "accounts",
        "type": "BTreeMap<AccountId, Account>",
        "meaning": "Shared account facts."
      },
      {
        "name": "Account.limits",
        "type": "Option<AsOf<Limits>>",
        "meaning": "Most recent stored limits for the account."
      }
    ],
    "usedBy": [
      "daemon::core::run_until_closed",
      "reduce::apply",
      "reduce::tick",
      "View::render"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 528
    },
    "status": "implemented"
  },
  {
    "id": "agent",
    "name": "Agent",
    "group": "State",
    "summary": "An Agent stores independent facts rather than a single mutable status flag: activity, prompts, children, waits, compaction, and placement. phase() combines those facts; shown remembers what has reached the screen. Several fields anticipate sources the daemon does not yet run.",
    "code": "pub struct Agent {\n    pub process: ProcKey,\n    pub provider: Provider,\n    pub life: Life,\n\n    pub session: Option<SessionId>,\n    pub account: AccountId,\n    pub title: Option<Box<str>>,\n\n    pub cwd: PathBuf,\n    pub workspace: Option<Box<str>>,\n    pub placement: Placement,\n    pub cpu_ticks: u64,\n\n    pub activity: Activity,\n    pub prompts: Prompts,\n    pub compaction: Compaction,\n    pub waits: BTreeMap<WaitId, Wait>,\n    pub subagents: Subagents,\n    pub beat: Option<AsOf<Beat>>,\n\n    pub shown: Shown,\n\n    pub last_event: Option<Instant>,\n}",
    "fields": [
      {
        "name": "process / provider / life",
        "type": "ProcKey / Provider / Life",
        "meaning": "Identity and lifecycle metadata. Died currently removes the map entry."
      },
      {
        "name": "session / account / title",
        "type": "Option<SessionId> / AccountId / Option<Box<str>>",
        "meaning": "Current conversation, shared limit owner, and optional display title."
      },
      {
        "name": "cwd / workspace / placement",
        "type": "PathBuf / Option<Box<str>> / Placement",
        "meaning": "Location facts kept separate from attention."
      },
      {
        "name": "cpu_ticks",
        "type": "u64",
        "meaning": "Process CPU accumulator refreshed by discovery sweeps."
      },
      {
        "name": "activity",
        "type": "Activity",
        "meaning": "Unobserved, idle, or an active turn with calls."
      },
      {
        "name": "prompts",
        "type": "Prompts",
        "meaning": "Requests that have not joined a call yet."
      },
      {
        "name": "compaction / waits / subagents",
        "type": "Compaction / BTreeMap<WaitId, Wait> / Subagents",
        "meaning": "Independent pieces of ongoing work."
      },
      {
        "name": "beat",
        "type": "Option<AsOf<Beat>>",
        "meaning": "Latest session heartbeat, when one is supplied."
      },
      {
        "name": "shown",
        "type": "Shown",
        "meaning": "Displayed phase plus any pending escalation."
      },
      {
        "name": "last_event",
        "type": "Option<Instant>",
        "meaning": "Timestamp used by some snapshot freshness checks."
      }
    ],
    "usedBy": [
      "Board.agents",
      "reduce::event::transition",
      "reduce::tick",
      "Agent::phase",
      "View::render"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 627
    },
    "status": "implemented"
  },
  {
    "id": "activity",
    "name": "Activity · TurnKey",
    "group": "State",
    "summary": "There is no standalone Turn struct. An active turn is the Activity::Turn variant, with a keyed map for parallel calls. Thinking is a turn with an empty tools map; an interrupted idle state retains its unfinished-work outcome.",
    "code": "pub enum Activity {\n    Unobserved { since: Instant },\n    Idle {\n        since: Instant,\n        after: Option<TurnOutcome>,\n    },\n    Turn {\n        key: Option<TurnKey>,\n        started: Instant,\n        tools: BTreeMap<ToolUseId, Tool>,\n        last_event: Instant,\n    },\n}\n\npub enum TurnKey {\n    Prompt(PromptId),\n    Turn(TurnId),\n}\n\npub enum TurnOutcome {\n    Completed,\n    Interrupted {\n        killed: Option<MessageId>,\n    },\n    Failed {\n        class: ErrorClass,\n    },\n}",
    "fields": [
      {
        "name": "Unobserved.since",
        "type": "Instant",
        "meaning": "Discovered without enough evidence to claim idle or working."
      },
      {
        "name": "Idle.after",
        "type": "Option<TurnOutcome>",
        "meaning": "Remembers completed, interrupted, or failed work until a later start."
      },
      {
        "name": "Turn.key",
        "type": "Option<TurnKey>",
        "meaning": "Optional identifier used to reject a late close for another turn."
      },
      {
        "name": "Turn.started / last_event",
        "type": "Instant",
        "meaning": "Turn age and staleness anchor."
      },
      {
        "name": "Turn.tools",
        "type": "BTreeMap<ToolUseId, Tool>",
        "meaning": "Concurrent outstanding calls, with each call carrying its gate."
      }
    ],
    "usedBy": [
      "Agent.activity",
      "reduce::event::transition",
      "reduce::tick",
      "Agent::phase"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 694
    },
    "status": "implemented"
  },
  {
    "id": "tool-gate",
    "name": "Tool · Gate · Ownership",
    "group": "State",
    "summary": "A gate belongs to the call it blocks. Human ownership can raise Attention; unknown ownership yields Unknown when no stronger rule applies. Machine-owned gates do not ask the person to act. This prevents a separate blocked flag drifting away from its call.",
    "code": "pub struct Tool {\n    pub name: ToolName,\n    pub hash: InputHash,\n    pub since: Instant,\n    pub gate: Gate,\n}\n\npub enum Gate {\n    Running,\n    Blocked {\n        kind: GateKind,\n        since: Instant,\n        owner: Ownership,\n        req: Option<RequestId>,\n    },\n}\n\npub enum GateKind {\n    Approval,\n    Question,\n}\n\npub enum Ownership {\n    Human,\n    Machine,\n    Unknown,\n}",
    "fields": [
      {
        "name": "Tool.name / hash",
        "type": "ToolName / InputHash",
        "meaning": "Tool identity and canonical pre-approval input hash for fallback joining."
      },
      {
        "name": "Tool.since",
        "type": "Instant",
        "meaning": "Local dispatch time."
      },
      {
        "name": "Tool.gate",
        "type": "Gate",
        "meaning": "Running or Blocked."
      },
      {
        "name": "Blocked.kind",
        "type": "GateKind",
        "meaning": "Approval asks permission; Question asks for an answer."
      },
      {
        "name": "Blocked.owner",
        "type": "Ownership",
        "meaning": "Human, Machine, or Unknown."
      },
      {
        "name": "Blocked.req",
        "type": "Option<RequestId>",
        "meaning": "Server approval request identifier, if available."
      }
    ],
    "usedBy": [
      "Activity::Turn.tools",
      "reduce::event",
      "Agent::wants_a_human",
      "Agent::has_unowned_gate"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 730
    },
    "status": "implemented"
  },
  {
    "id": "prompts",
    "name": "Prompts · Unjoined",
    "group": "State",
    "summary": "Hooks run as separate processes, so a request can reach the daemon before its call. The join works in both directions: keep the request here until a dispatch with matching actor, tool, and input hash arrives. These unmatched requests can themselves affect the phase.",
    "code": "pub struct Prompts {\n    pub unjoined: Vec<Unjoined>,\n}\n\npub struct Unjoined {\n    pub actor: Actor,\n    pub tool: ToolName,\n    pub hash: Option<InputHash>,\n    pub since: Instant,\n    pub owner: Ownership,\n    pub kind: GateKind,\n}",
    "fields": [
      {
        "name": "unjoined",
        "type": "Vec<Unjoined>",
        "meaning": "Requests waiting for a matching call, with a 30-minute leak-bound expiry."
      },
      {
        "name": "actor / tool",
        "type": "Actor / ToolName",
        "meaning": "Restricts the request to the entity and tool that caused it."
      },
      {
        "name": "hash",
        "type": "Option<InputHash>",
        "meaning": "Pre-approval input fingerprint when supplied."
      },
      {
        "name": "since",
        "type": "Instant",
        "meaning": "Arrival time and expiry anchor."
      },
      {
        "name": "owner / kind",
        "type": "Ownership / GateKind",
        "meaning": "Who must respond and whether this is approval or a question."
      }
    ],
    "usedBy": [
      "Agent.prompts",
      "reduce::event",
      "reduce::tick",
      "Agent::phase"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 778
    },
    "status": "implemented"
  },
  {
    "id": "subagents",
    "name": "Subagents · Subagent",
    "group": "State",
    "summary": "A flat child table lives under each main agent. Hook-created rows are implemented; whole-panel and directory source modes exist without runtime producers. Each child currently has one optional gate, which is the audited flaw behind the proposed per-call child map.",
    "code": "pub struct Subagents {\n    pub rows: BTreeMap<SubagentId, Subagent>,\n    pub as_of: Option<Instant>,\n    pub source: SubagentSource,\n}\n\npub struct Subagent {\n    pub kind: SubagentKind,\n    pub status: SubagentStatus,\n    pub parent: Option<SubagentId>,\n    pub started: SystemTime,\n    pub label: Box<str>,\n    pub tokens: u64,\n    pub gate: Option<Gate>,\n    pub last_seen: Instant,\n}\n\npub enum SubagentSource {\n    Panel,\n    DirScan,\n    #[default]\n    EventsOnly,\n}\n\npub enum SubagentStatus {\n    Pending,\n    Running,\n    Completed,\n    Failed,\n    Killed,\n    Paused,\n}",
    "fields": [
      {
        "name": "rows",
        "type": "BTreeMap<SubagentId, Subagent>",
        "meaning": "Child identities to child facts; parent carries depth without recursive Agent values."
      },
      {
        "name": "as_of",
        "type": "Option<Instant>",
        "meaning": "Most recent whole-table observation time."
      },
      {
        "name": "source",
        "type": "SubagentSource",
        "meaning": "Panel, DirScan, or EventsOnly; daemon discovery starts EventsOnly."
      },
      {
        "name": "status",
        "type": "SubagentStatus",
        "meaning": "Running and Pending contribute Delegating when the main turn is idle."
      },
      {
        "name": "parent / label / tokens",
        "type": "Option<SubagentId> / Box<str> / u64",
        "meaning": "Relationship and panel metadata."
      },
      {
        "name": "gate",
        "type": "Option<Gate>",
        "meaning": "One gate per child today; human gates propagate attention to the main chip."
      },
      {
        "name": "last_seen",
        "type": "Instant",
        "meaning": "Last child observation; the desired stale-cap policy is incomplete."
      }
    ],
    "usedBy": [
      "Agent.subagents",
      "reduce::event",
      "reduce::snapshot::replace_panel",
      "Agent::phase"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 852
    },
    "status": "implemented"
  },
  {
    "id": "limits",
    "name": "Limits · LimitStatus",
    "group": "State",
    "summary": "Usage status is stored once per account and consulted during each agent phase calculation. Rejected causes Blocked. resets_at exists as a wall-clock deadline, but current tick does not expire it; that is a confirmed audit bug, not a completed timer feature.",
    "code": "pub struct Limits {\n    pub status: LimitStatus,\n    pub resets_at: Option<SystemTime>,\n    pub window: Option<LimitWindow>,\n    pub used_percent: Option<u8>,\n}\n\npub enum LimitStatus {\n    Allowed,\n    Warning,\n    Rejected,\n}\n\npub enum LimitWindow {\n    FiveHour,\n    SevenDay,\n    Overage,\n}",
    "fields": [
      {
        "name": "status",
        "type": "LimitStatus",
        "meaning": "Allowed, Warning, or Rejected; only Rejected blocks."
      },
      {
        "name": "resets_at",
        "type": "Option<SystemTime>",
        "meaning": "Provider wall-clock reset deadline; currently used for a reason string rather than expiry."
      },
      {
        "name": "window",
        "type": "Option<LimitWindow>",
        "meaning": "Which usage window the observation describes."
      },
      {
        "name": "used_percent",
        "type": "Option<u8>",
        "meaning": "Optional utilization percentage."
      }
    ],
    "usedBy": [
      "SnapshotKind::Limits",
      "Account.limits",
      "reduce::limits_for",
      "Agent::phase",
      "view::reason"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 558
    },
    "status": "implemented"
  },
  {
    "id": "phase",
    "name": "Phase · Severity · Lamp",
    "group": "Derivation",
    "summary": "The truth is derived by Agent::phase in a fixed order: human gate, rejected account or failed turn, interrupted turn, unowned gate, active turn, unobserved, running child, outstanding wait, idle. Colour and interruption severity are separate axes.",
    "code": "pub enum Phase {\n    Attention,\n    Blocked,\n    Stopped,\n    Unknown,\n    Working,\n    Delegating,\n    Waiting,\n    Idle,\n}\n\npub enum Severity {\n    Calm,\n    Notice,\n    Alert,\n}\n\npub enum Lamp {\n    Grey,\n    Green,\n    Amber,\n    Red,\n}",
    "fields": [
      {
        "name": "Attention",
        "type": "Phase → Red / Alert",
        "meaning": "A human must answer a gate."
      },
      {
        "name": "Blocked / Stopped",
        "type": "Phase → Amber / Notice",
        "meaning": "External failure or unfinished work after interruption."
      },
      {
        "name": "Unknown",
        "type": "Phase → Grey / Calm",
        "meaning": "No sufficient observation, or a gate with unknown owner."
      },
      {
        "name": "Working / Delegating / Waiting / Idle",
        "type": "Phase → Green / Calm",
        "meaning": "Distinct work states sharing a quiet colour."
      },
      {
        "name": "severity()",
        "type": "Severity",
        "meaning": "Orders escalation for the display dwell rules."
      },
      {
        "name": "lamp()",
        "type": "Lamp",
        "meaning": "Chooses the visible colour, without changing model facts."
      }
    ],
    "usedBy": [
      "Agent::phase",
      "Shown::settle",
      "View::render",
      "view::lamp_name"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 924
    },
    "status": "implemented"
  },
  {
    "id": "shown",
    "name": "Shown · Escalation",
    "group": "Derivation",
    "summary": "The display has memory even though truth is recomputed. Quieter changes, user-caused changes, and confirmed evidence show immediately. Agent-caused escalation waits 10 seconds for inferred evidence or 6 seconds for snapshots. Evidence is taken from the arriving event today; Verdict is a proposed fix.",
    "code": "pub struct Shown {\n    pub phase: Phase,\n    pub since: Instant,\n    pub pending: Option<Escalation>,\n}\n\npub struct Escalation {\n    pub to: Phase,\n    pub first_seen: Instant,\n    pub promote_at: Instant,\n}",
    "fields": [
      {
        "name": "phase",
        "type": "Phase",
        "meaning": "What the chip currently displays."
      },
      {
        "name": "since",
        "type": "Instant",
        "meaning": "When the displayed phase last changed."
      },
      {
        "name": "pending",
        "type": "Option<Escalation>",
        "meaning": "A louder candidate that has not served its dwell yet."
      },
      {
        "name": "Escalation.to",
        "type": "Phase",
        "meaning": "Target displayed phase."
      },
      {
        "name": "first_seen / promote_at",
        "type": "Instant",
        "meaning": "Candidate arrival and promotion deadline; next_deadline schedules the core wakeup."
      }
    ],
    "usedBy": [
      "Agent.shown",
      "reduce::settle",
      "reduce::next_deadline",
      "reduce::tick",
      "AgentView.shown"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 1004
    },
    "status": "implemented"
  },
  {
    "id": "effect",
    "name": "Effect",
    "group": "Output",
    "summary": "The synchronous reducer returns requests for external work. The core handles PublishState itself; an unbounded channel carries the other effects to the runner. A watcher can learn new facts and send them back as Input, completing the feedback loop.",
    "code": "pub enum Effect {\n    PublishState,\n    AppendHistory(HistoryEntry),\n    WatchTranscript {\n        process: ProcKey,\n        session: SessionId,\n        cwd: PathBuf,\n    },\n    Forget { process: ProcKey },\n}",
    "fields": [
      {
        "name": "PublishState",
        "type": "variant",
        "meaning": "Render and compare a View; update the watch channel only if unequal."
      },
      {
        "name": "AppendHistory",
        "type": "HistoryEntry",
        "meaning": "Send a lifecycle or displayed-phase edge to the history appender."
      },
      {
        "name": "WatchTranscript",
        "type": "{ process, session, cwd }",
        "meaning": "Replace the process watcher; the runner still derives a Claude transcript path from cwd/session."
      },
      {
        "name": "Forget",
        "type": "{ process }",
        "meaning": "Abort and remove the process watcher after death."
      }
    ],
    "usedBy": [
      "reduce::Effects",
      "daemon::core::dispatch",
      "daemon::effects::run",
      "daemon::watcher"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 1308
    },
    "status": "implemented"
  },
  {
    "id": "view",
    "name": "View · AgentView",
    "group": "Output",
    "summary": "The read model is an immutable Arc<View> on a latest-value watch channel. Whole views are compared before subscribers wake. Both state.json and socket responses serialize the same schema, switchboard-rust/1. The reason is eagerly computed and separately derived from the truth.",
    "code": "pub struct View {\n    pub agents: Vec<AgentView>,\n}\n\npub struct AgentView {\n    pub process: ProcKey,\n    pub session: Option<SessionId>,\n    pub name: String,\n    pub cwd: PathBuf,\n    pub shown: Phase,\n    pub since: Instant,\n    pub truth: Phase,\n    pub reason: String,\n    pub focused: bool,\n}",
    "fields": [
      {
        "name": "agents",
        "type": "Vec<AgentView>",
        "meaning": "Stable ordered projection of the current local agents."
      },
      {
        "name": "process / session",
        "type": "ProcKey / Option<SessionId>",
        "meaning": "Identity becomes pid, start, and session_id in JSON."
      },
      {
        "name": "name / cwd",
        "type": "String / PathBuf",
        "meaning": "Already-derived display label and working directory."
      },
      {
        "name": "shown / truth",
        "type": "Phase / Phase",
        "meaning": "Displayed phase may lag the current derived phase."
      },
      {
        "name": "since",
        "type": "Instant",
        "meaning": "Converted to epoch since_ms when serializing."
      },
      {
        "name": "reason",
        "type": "String",
        "meaning": "Eager tooltip text; current independent reason logic has audited inconsistencies."
      },
      {
        "name": "focused",
        "type": "bool",
        "meaning": "From placement; no live placement producer is wired today."
      }
    ],
    "usedBy": [
      "View::render",
      "daemon::core::publish",
      "daemon::publish::run",
      "daemon::rpc",
      "client::subscribe"
    ],
    "source": {
      "path": "src/view.rs",
      "line": 21
    },
    "status": "implemented"
  },
  {
    "id": "history",
    "name": "HistoryEntry · Record",
    "group": "Output",
    "summary": "History records lifecycle and displayed-phase edges, not every raw input or complete board. The appender adds wall time and writes daily JSON lines. History is best effort: its bounded queue can drop entries when full.",
    "code": "pub enum HistoryEntry {\n    AgentFound {\n        process: ProcKey,\n        cwd: PathBuf,\n    },\n    AgentGone {\n        process: ProcKey,\n        session: Option<SessionId>,\n    },\n    Shown {\n        process: ProcKey,\n        session: Option<SessionId>,\n        from: Phase,\n        to: Phase,\n        held: Duration,\n    },\n}\n\npub struct Record {\n    pub at: SystemTime,\n    pub pid: u32,\n    pub session: Option<SessionId>,\n    pub kind: RecordKind,\n}\n\npub enum RecordKind {\n    SessionStart {\n        cwd: PathBuf,\n    },\n    SessionEnd,\n    Shown {\n        from: Phase,\n        to: Phase,\n        held: Duration,\n    },\n}",
    "fields": [
      {
        "name": "AgentFound / AgentGone",
        "type": "HistoryEntry variants",
        "meaning": "Local process discovery and removal, serialized as session_start/session_end records."
      },
      {
        "name": "Shown",
        "type": "{ process, session, from, to, held }",
        "meaning": "The displayed phase changed; held measures how long the previous display lasted."
      },
      {
        "name": "Record.at",
        "type": "SystemTime",
        "meaning": "Appended wall-clock timestamp."
      },
      {
        "name": "Record.pid / session",
        "type": "u32 / Option<SessionId>",
        "meaning": "History identity; the serialized record does not retain ProcKey.start."
      },
      {
        "name": "Record.kind",
        "type": "RecordKind",
        "meaning": "The compact disk vocabulary."
      }
    ],
    "usedBy": [
      "reduce::settle",
      "Effect::AppendHistory",
      "daemon::history::run",
      "Record::from_entry",
      "HistoryWriter"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 1335
    },
    "status": "implemented"
  },
  {
    "id": "fleet",
    "name": "Fleet · Row · Reach",
    "group": "Extension",
    "summary": "These real declarations describe a separate merged local/remote view, but no daemon task currently feeds or publishes a Fleet. Remote rows carry viewer-relative reachability and freshness; a stale source paints grey. This is an extension sketch, not an active network path.",
    "code": "pub struct Fleet {\n    pub local: Board,\n    pub remote: BTreeMap<Host, AsOf<Vec<Row>>>,\n}\n\npub struct Row {\n    pub key: RowKey,\n    pub provider: Provider,\n    pub phase: Phase,\n    pub reach: Reach,\n    pub freshness: Freshness,\n    pub workspace: Option<Box<str>>,\n    pub title: Option<Box<str>>,\n}\n\npub struct RowKey {\n    pub host: Host,\n    pub pid: u32,\n    pub started_at: SystemTime,\n}\n\npub enum Reach {\n    Here { placement: Placement },\n    ViaHost { host: Host, local: Option<PaneKey> },\n    Unreachable,\n}\n\npub enum Freshness {\n    Live,\n    Unconfirmed { since: Instant },\n}",
    "fields": [
      {
        "name": "local",
        "type": "Board",
        "meaning": "The local live model; only this side owns process discovery."
      },
      {
        "name": "remote",
        "type": "BTreeMap<Host, AsOf<Vec<Row>>>",
        "meaning": "A whole row snapshot per remote host."
      },
      {
        "name": "Row.key",
        "type": "RowKey",
        "meaning": "Host, pid, and discovery wall time; not the same guarantee as ProcKey."
      },
      {
        "name": "Row.phase",
        "type": "Phase",
        "meaning": "The remote source’s reported phase."
      },
      {
        "name": "Row.reach",
        "type": "Reach",
        "meaning": "Act here, through a local connection to a host, or unreachable."
      },
      {
        "name": "Row.freshness",
        "type": "Freshness",
        "meaning": "Unconfirmed rows render grey without inventing a new remote phase."
      }
    ],
    "usedBy": [
      "model declarations",
      "Row::lamp",
      "No daemon producer or publisher"
    ],
    "source": {
      "path": "src/model.rs",
      "line": 1803
    },
    "status": "unwired"
  }
];
