# Switchboard in Rust — rewrite plan (draft 1, 2026-09-10)

This is the synthesis of the surveys in `docs/survey/` (index in
`docs/survey/README.md`; 01–08 cover the Go daemon and the prior Rust attempt,
09–14 cover the signals, transcripts, hooks, and the two installed binaries).
The concurrency argument behind §3 is in `docs/01-concurrency-design.md`. It is
a proposal to iterate on, not a spec. Every section ends with the decisions it
leaves open.

## 0. Ground truth

| Fact | Value |
|---|---|
| Go daemon, non-test / test LOC (`cmd/` + `internal/`) | 36k / 40k |
| Age, commits | started 2026-05-28, 422 commits on `main` |
| Live stack on this machine | Hyprland + wezterm, Claude + Codex, `-remote nlessfun`, bottombar attached publisher, dashboard, history `full` tier |
| Prior Rust attempt | `~/Projects/agent-watcher`: 15 crates, 67k LOC (44k non-test), abandoned 2026-08-14; builds and passes 893 tests; pidfd watch and supervisor never wired into `main` |
| Rewrite docs in the Go repo | `rust-rewrite-kickoff.md`, `rust-rewrite-requirements-v0.2.md` (2026-08-12/24). Stale: they target the `writerstate` FSM branch that never merged; the shipped model is `internal/agentgraph` (since 2026-08-21) |
| Essential Linux daemon content, by the post-mortem's estimate | 3–5k LOC |

The two things the surveys agree on most strongly:

1. **The value of the Go code is in a small number of hard-won rules**, not in
   its architecture. The rules are enumerated in survey 07 §2–§3 (36 decisions,
   ~30 hazards) and survey 03 §2 (the status transition tables). They port as
   tests and enums; the architecture does not need to port at all.
2. **The prior Rust attempt failed by building for a team that does not exist**:
   conformance suites for absent backends, blind macOS/Windows code, 19k lines of
   markdown. This plan sizes for one user, one OS, one WM, one terminal, and
   keeps the owner writing the spine.

## 1. What the daemon actually does (one paragraph per subsystem)

- **Discovery** (survey 02). Scan `/proc` once a second; a process is a session
  iff `classify({comm, exe, argv})` says Claude or Codex (exact rules in 02 §4.5).
  Open a pidfd per session; death is observed, never inferred. Liveness is
  three-valued (alive / gone / unknown). A per-tick sweep backstops the watch.
  Exactly one `session_end` per death.
- **Status** (survey 03). Each root has an *agent graph*: a tree of nodes, each
  with three orthogonal axes (runtime, attention, lifecycle). A pure fold reduces
  the graph plus a freshness deadline to one chip color: red (a human-owned
  request is waiting) > green (root active, or a live child working) > orange
  (idle) > grey (cannot tell). Claude evidence: eight hooks plus transcript tails.
  Codex evidence: eight hooks, optionally a `codex app-server` child. Per-writer
  prompt ownership, latching of call ids, and the missed-RED guards are where all
  the subtlety is.
- **Navigate** (survey 04). tty joins process → pane (wezterm CLI or tmux);
  `(mux pid, [sbw:gui:window] marker)` joins pane → WM window (Hyprland IPC).
  Ambiguity returns nothing and retries next tick. Focus = raise window + activate
  pane. Attention ring and cycle are pure functions over a snapshot.
- **Store and wire** (survey 01). One session map, one mutation path, deep-copied
  snapshots, a change key that suppresses no-op publishes, atomic `state.json`,
  JSONL RPC on a Unix socket (12 commands), hydrate-on-restart rules.
- **Clients** (survey 05). `switchboard-ctl` with 22 subcommands; the hook
  forwarder (privacy boundary: hashes tool input, `getppid()` attribution, never
  fails); the Waybar broker writing ten atomic slot files and signalling via
  pidfd; TUI; polybar.
- **History and federation** (survey 06). Opt-in JSONL activity log, one file per
  day, two privacy tiers; the dashboard consumes `switchboard-ctl timeline --json`
  from it. Federation pulls a peer's snapshots over `ssh` and binds remote
  sessions to local wezterm panes via an OSC user-var; never proven live.

## 2. Scope for v1 (proposed)

Keep what is live-verified on this machine and what the rules need. Drop what
was built for other machines, other people, or incidents that a rewrite
designs out.

| Keep in v1 | Defer | Drop |
|---|---|---|
| Linux `/proc` + pidfd | macOS (kqueue), Windows: keep the `ProcessSource` trait seam, no code | blind backends |
| Hyprland (hand-rolled IPC) | sway/i3 via `swayipc-async` | X11/EWMH (verified only against XWayland) |
| wezterm CLI + `[sbw:…]` marker join | tmux (pane focus only, no WM raise) | legacy title-equality join |
| Claude: hooks + transcript + subagents dir | — | legacy reconciler, `rpc.handleHook` status path, shadow/compat machinery |
| Codex: hook-only reducer (question/approval records, 30 s grace) | `codex app-server` observer (topology, auto-review classification), display-name generation | wrapper/slot designs (already removed in Go) |
| `state.json` schema v3, JSON-equal | byte-equal encoder | `mem_*` fields (never shipped) |
| RPC: `list`, `subscribe`, `focus-session`, `hook`, `activity` | `list-all`/`subscribe-all`, pane-bind/state, announce-bindings | legacy `focus` selector command |
| ctl: `list`, `pick`, `focus`, `cycle`, `attention`, `hook`, `codex-hook`, `bottombar publish` (attached), `tui` | `diagnose` | polybar, standalone waybar, legacy `bottombar watch/reconcile/stop`, `seed-bench`, `name` |
| History sink writing the same JSONL schema | Rust `timeline` reader | pricing table, `calibrate`, memory samples, repair scripts |
| — | Federation (observe-only first) | remote hysteresis, pane-bind registry until a second machine is available |

Two scope moves that buy the most:

- **The dashboard keeps working without porting the timeline reader.** It calls
  `switchboard-ctl timeline --json` over a process boundary and reads only the
  history day-files. If the Rust daemon writes the same JSONL, the Go
  `switchboard-ctl timeline` binary stays as the provider until a Rust port
  exists. That removes ~2k lines (timeline + suspect + carryover) from v1.
- **Codex hook-only.** The app-server observer plus its protocol is 2.4k lines
  of Go for child topology and auto-review ownership. The hook reducer alone
  (survey 03 §2.8) gives working/idle, immediate red on `request_user_input`, and
  approval red after a 30 s grace. That is the FR-E8a path the requirements call
  "the supported path". Add the observer in v2 if grey Codex children matter.

Open: (a) keep `state.json` v3 or break it; (b) Codex hook-only; (c) Go
`timeline` as the dashboard provider during migration. Recommendations are the
table above.

## 3. Shape: one crate, two binaries, one actor

```
switchboard-rust/
  Cargo.toml            [[bin]] switchboard, [[bin]] switchboard-ctl; one lib
  src/
    lib.rs
    main.rs             daemon: clap, tracing, signals → CancellationToken, wiring
    bin/ctl.rs          switchboard-ctl: sync std::net, no tokio on the hook path
    proc/               ProcId, ProcessInfo, Liveness, trait ProcessSource,
                        linux.rs (/proc parsers, pure fns over &str), pidfd.rs (AsyncFd)
    discovery.rs        classify(), is_headless(), codex argv parser  [port Go tables]
    model/              Session, Snapshot, Capabilities, serde = wire (field order = struct order)
    graph/              Runtime, Attention, Lifecycle, Node, Observation, Summary,
                        normalize(), reduce()                          [port Go truth tables]
    provider/
      mod.rs            Writer, RootKey, HookEvent, trait Provider (produces events)
      claude/           hooks.rs, prompts.rs (per-writer set + latch), resolve.rs,
                        transcript.rs (tail + entry shapes), fanout.rs (subagents dir)
      codex/            hooks.rs (question/approval records, grace timer as an event)
    core.rs             the actor: owns HashMap<ProcId, SessionState>; fn apply(&mut self, Event) -> Vec<Effect>
    store.rs            Snapshot projection, change key, watch::Sender<Arc<Snapshot>>
    publish.rs          state.json writer (temp+rename, latest wins)
    navigate/
      wm.rs             trait WindowManager, WmEvent, none
      hyprland.rs       socket paths, instance discovery, request, socket2 reader, Lua focus batch
      terminal.rs       trait Terminal, PaneRef, none
      wezterm.rs        gui-sock discovery, `cli list` parse, activate
      join.rs           pure: join(pane, clients) -> Option<Window>; title::normalize
      focus.rs          pure: attention ring, cycle, selector parsing
    rpc/                listener, per-connection task, Request/Response, hook attribution (ppid walk)
    history/            Event (flat JSON), Sink (mpsc + blocking writer), config, retention
    render/             barview (status/classes), barlayout (Fit), waybar chip, tui frame
    bottombar.rs        attached publisher: slot files, ready handshake, pidfd signal (linux only)
```

Crate policy, applied uniformly: `tokio` (rt-multi-thread, net, time, signal,
process, io-util, sync, macros), `tokio-util` (CancellationToken), `serde`,
`serde_json`, `rustix` (pidfd, poll, getppid, flock, tty open), `clap`,
`tracing` + `tracing-subscriber`, `thiserror`, `jiff` (RFC 3339 and local-day
keys for history). Nothing else without a reason written down. No `procfs`,
no `hyprland-rs`, no `ratatui` (the TUI is CRLF lines).

Target: 3–5k LOC of source, roughly the same in tests, most of which are
ported Go table tests.

### 3.1 The actor

The Go daemon is a `RwMutex` store plus lock-order discipline plus three
kinds of fence (`broadcastGen`, `resolveTurn`, per-root `generation`) because
several goroutines mutate one map. The Rust design puts all mutable daemon
state behind **one task** that owns it outright. Everything else produces
`Event`s into its inbox or consumes `Arc<Snapshot>`s from its outbox.

```
                    ┌────────────────────────────────────────────────────┐
   scanner (1 s) ──►│                                                    │
   deathwatch[n] ──►│  core task                                         │──► watch<Arc<Snapshot>>
   wm_events     ──►│   state: HashMap<ProcId, SessionState>             │       ├─► publisher (state.json)
   resolver (5 s)──►│   fn apply(&mut self, Event) -> Vec<Effect>        │       ├─► rpc subscribe streams
   observer      ──►│   pure except for the channel sends                │       ├─► scanner (known set)
   rpc: hook     ──►│                                                    │       └─► bottombar / tui / ctl
   timer wheel   ──►│                                                    │──► mpsc<history::Event>
   signals       ──►└────────────────────────────────────────────────────┘──► mpsc<Invalidate(RootKey)> → observer
                                                                          ──► spawn/abort deathwatch tasks
```

- **No I/O in `apply`.** Every read (`/proc`, transcript tails, `wezterm cli
  list`, Hyprland clients) happens in a producer task and arrives as data.
  This is the Go rule "no I/O under the store lock" made structural.
- **No fences.** Ordering is the inbox order. A producer that can be stale
  (the resolver) stamps its result with the sequence it observed; the core
  drops results older than the last applied one. That is `resolveTurn` and
  `broadcastGen` in one comparison.
- **Timers are events.** The core returns `Effect::Deadline { key, purpose,
  at, generation }`; a small timer task sleeps and sends `Event::Deadline`
  back. The core ignores a deadline whose generation no longer matches. Every
  Go `time.AfterFunc` with its bespoke fencing (survey 03 §4.2, four kinds)
  collapses into this, and the reducer becomes testable without sleeping.
- **Reads never touch the core.** `list`, renderers, `focus` all read the
  `watch` receiver. `focus` performs its own WM/terminal I/O from the snapshot.

What is async, and why:

| Waits on | Task | Mechanism |
|---|---|---|
| Unix socket accept / read / write | rpc, per-connection | `tokio::net::UnixListener` |
| Process exit | deathwatch, one per session | `AsyncFd<OwnedFd>` on `pidfd_open`; drop = stop |
| Hyprland event stream | wm_events | `UnixStream` + `BufReader::lines`, reconnect with backoff |
| Ticks and deadlines | scanner, resolver, timer wheel | `tokio::time::{interval, sleep}` |
| SIGTERM / SIGINT | signals | `tokio::signal::unix` → `CancellationToken` |
| `wezterm cli list`, later `codex app-server` | resolver, codex observer | `tokio::process::Command` + `timeout` |
| Subscribers waking | rpc subscribe, publisher, bottombar | `watch::Receiver::changed()` |

What is sync, and why:

| Work | Where | Why sync |
|---|---|---|
| `/proc` reads and parsing | scanner, sweep | microseconds; pure fns over `&str`; `spawn_blocking` only if a profile says so |
| Transcript tail reads (128 KiB, bounded) | observer | small, bounded; `std::fs` inside `spawn_blocking` or a dedicated thread |
| `apply`, `reduce`, prompt/latch rules, join, attention ring | core, pure modules | no waiting; the whole point is that they are unit-testable tables |
| Snapshot projection + change key + JSON encode | core | sub-millisecond; done once per publish |
| `state.json` write | publisher | `spawn_blocking`, latest wins |
| History append | sink thread | `try_send` from the core (drop on full), one blocking writer |
| Everything in `switchboard-ctl` | ctl binary | one-shots; the hook path must start in microseconds and never spin up a runtime |

Open: actor core (recommended) versus a `Mutex<Store>` with `spawn_blocking`
ticks (survey 01's sketch, closer to Go). The actor is more Rust-shaped and
removes three fence mechanisms; the mutex version is easier to port
line-by-line.

## 4. Types (first cut)

Identity and process layer:

```rust
pub struct Pid(pub i32);
/// pid + starttime (/proc/<pid>/stat field 22). Equality == "same process" everywhere.
#[derive(Copy, Clone, PartialEq, Eq, Hash)]
pub struct ProcId { pub pid: Pid, pub start_ticks: u64 }

pub struct ProcessInfo {
    pub id: ProcId, pub ppid: Pid, pub comm: String, pub state: RunState,
    pub exe: Option<PathBuf>, pub cwd: Option<PathBuf>,   // None = masked, never an error
    pub tty: Option<Tty>, pub args: Option<Vec<OsString>>,
}
pub enum ReadError { Gone, Unsupported, Io(std::io::Error) }
pub enum Liveness { Alive(ProcessInfo), Gone, Unknown(ReadError) }   // never collapse Unknown into Gone

pub trait ProcessSource: Send + Sync {
    fn list_pids(&self) -> Result<Vec<Pid>, ReadError>;
    fn read(&self, pid: Pid) -> Result<ProcessInfo, ReadError>;
    fn watch(&self, id: ProcId) -> Result<DeathHandle, ReadError>;  // Future<Output=()>; drop cancels
}

pub enum Agent { Claude, Codex }
pub struct Classified { pub agent: Agent, pub headless: bool }
pub fn classify(info: &ProcessInfo) -> Option<Classified>;          // port discovery_test.go tables
```

Keying sessions on `ProcId` rather than `pid` dissolves the Go scanner's
seen-set and `Forget`, closes the zombie-rediscovery loop (reject `state == Z`),
and makes pid reuse a different key rather than a fence.

Agent graph (port `internal/agentgraph` verbatim; it is finished):

```rust
#[serde(rename_all = "snake_case")]
pub enum Runtime   { Unknown, NotLoaded, Idle, Active, SystemError }
pub enum Attention { None, Approval, UserInput }
pub enum Lifecycle { Unknown, Pending, Running, Completed, Interrupted, Errored, Shutdown, NotFound }
pub enum Status    { Working, Idle, Permission, Delegating }        // Option<Status>: None == "" == grey

pub struct Node { id: NodeId, parent: Option<NodeId>, nickname: String, role: String,
                  runtime: Runtime, attention: Attention, lifecycle: Lifecycle,
                  started_at: Option<Timestamp>, updated_at: Option<Timestamp>, completed_at: Option<Timestamp> }
pub struct Observation { root: NodeId, source: Source, observed_at: Timestamp, fresh_until: Timestamp,
                         complete: bool, nodes: Vec<Node> }
pub struct Summary { runtime, attention, status: Option<Status>, live_children, waiting_nodes, ... since }
pub fn normalize(obs: Observation) -> Result<Observation, GraphError>;   // unique ids, one root, acyclic, DFS order
pub fn reduce(obs: &Observation, prior: Option<&Summary>, now: Timestamp) -> Summary;
```

The fold (survey 03 §2.2): not fresh → grey; waiting > 0 → permission; root
active → working; root system_error → grey; live child active/pending/running
→ delegating; root idle → idle; else grey.

The core's inbox:

```rust
pub enum Event {
    Appeared(ProcessInfo, Classified),
    Died(ProcId),
    Sweep(Vec<(ProcId, Liveness)>),                 // per-tick backstop
    Enumerated { seq: u64, panes: HashMap<Tty, PaneRef>, clients: Vec<Window>, active: Option<WindowRef> },
    Wm(WmEvent),                                     // FocusChanged | WindowClosed | LayoutChanged
    Hook(HookEvent),                                 // exact fields only; from switchboard-ctl
    Observed { root: RootKey, seq: u64, delta: ProviderDelta },  // transcript deltas, fanout facts
    Deadline { key: RootKey, purpose: Purpose, generation: u64 },
    Activity(Activity),
    Shutdown,
}
pub enum Effect {
    Publish,                                          // core projects a Snapshot, sends on watch
    History(history::Event),
    Invalidate(RootKey),                              // ask the observer to re-read this root
    Deadline { key: RootKey, purpose: Purpose, at: Instant, generation: u64 },
    Watch(ProcId), Unwatch(ProcId),
}
```

Session state as the core holds it (wire projection is derived):

```rust
pub struct SessionState {
    pub id: ProcId, pub agent: Agent, pub headless: bool, pub started_at: Timestamp,
    pub cwd: PathBuf, pub tty: Option<Tty>, pub suspended: bool, pub focused: bool,
    pub pane: Option<PaneRef>, pub window: Option<Window>,
    pub provider: ProviderState,                      // Claude(ClaudeRoot) | Codex(CodexRoot)
    pub graph: Option<(Observation, Summary)>,
}
```

Provider seams, deliberately narrow:

```rust
pub enum Writer { Main, Agent(String) }               // "" vs "agent-x" as a type, not a convention
pub struct HookEvent { root: RootKey, writer: Writer, at: Timestamp, kind: HookKind }
pub enum HookKind { SessionStart{source}, UserPromptSubmit, PreToolUse{tool, hash, call}, PermissionRequest{tool, hash},
                    PostToolUse{tool, hash, call}, Stop, SubagentStart, SubagentStop }
pub enum Latch { Unbound, Proposed{call, at}, Bound(CallId), Ambiguous, Contested }
pub struct PendingPrompt { tool: ToolName, hash: Option<InputHash>, attention: Attention, since: Timestamp, latch: Latch }
pub struct ClaudeRoot { runtime: Runtime, runtime_at: Timestamp, pending: BTreeMap<Writer, Vec<PendingPrompt>>,
                        anchors: BTreeMap<Writer, Timestamp>, children: fanout::State }
```

**Hook set (2026-09-11 finding).** Claude Code 2.1.268 emits far more events
than the eight the Go daemon wires, verified by reading the emitters in the
installed binary. Wire for v1: `SessionStart{source, model, session_title,
agent_type}`, `UserPromptSubmit{prompt, session_title}`, `PreToolUse{tool_name,
tool_input, tool_use_id}` **unrestricted**, `PermissionRequest{tool_name,
tool_input}` (still no `tool_use_id`), `PermissionDenied{tool_use_id, reason}`,
`PostToolUse{tool_use_id, duration_ms}`, `PostToolUseFailure{tool_use_id,
is_interrupt, error}`, `Stop{last_assistant_message}`, `StopFailure{error}`,
`Notification{notification_type}`, `Elicitation`/`ElicitationResult`,
`SubagentStart{agent_id, agent_type}`, `SubagentStop{agent_id,
agent_transcript_path}`, `CwdChanged{old_cwd, new_cwd}`, `PreCompact`/`PostCompact`,
`SessionEnd{reason}`. Never `MessageDisplay` (per delta) or `PostToolBatch`.
Consequence: with an id on every `PreToolUse`, the prompt-to-call join is
one step and the transcript latch (survey 03 §2.4, `Latch` states) is
unnecessary; `PermissionDenied`/`PostToolUseFailure` give exact release edges
for declines and interrupts. Still to measure live before relying on them:
does `PermissionDenied` fire on a human decline; does `Stop` fire on Esc;
when does `idle_prompt` fire; does a pre-first-token double-Esc emit anything.
Codex 0.154 emits twelve: the Go daemon's eight plus `Interrupt` (root only;
the release edge that ends a turn and clears pending records, so a Ctrl-C at an
approval prompt no longer stays red until the next prompt), `PreCompact`/
`PostCompact` (`compaction_trigger` manual|auto), and `SessionEnd`
(`end_reason`, currently only `other`). Codex still has no `PermissionDenied`,
`PostToolUseFailure`, `Notification`, `Elicitation`, or `CwdChanged`; approval
ownership stays an app-server or grace-timer question. Eleven events are
common to both agents; model `HookKind` as one enum with a Claude-only
extension rather than two vocabularies.

**Transcript, waits, children, blocks (2026-09-11 investigations, surveys 10–13).**
Four additions to the Claude root model, each replacing inference with a
record Claude Code already writes:

1. *Turn structure from disk is exact.* `system/stop_hook_summary` lands
   100 ms after the last block and is the turn end; `user.origin.kind`
   (`human` | `task-notification` | `peer` | `coordinator` |
   `auto-continuation`) says who started a turn. Lines land ~50 ms after
   their own timestamp on 2.1.268, so the "5 s flush lag" premise behind
   H7/H8 no longer holds; keep the wall-clock anchoring rule anyway, it is
   cheap and still correct. `stop_reason == end_turn` on a block is not a
   turn end (it appears on the thinking block up to 32 s early). Dedupe
   `usage` by `requestId`, never per line.
2. *Waiting.* `Stop`/`SubagentStop` carry `background_tasks[]` and
   `session_crons[]`; `UserPromptSubmit` carries `source`
   (`user|sdk|system|loop_wakeup|schedule_wakeup`). `ClaudeRoot` gains
   `waits: BTreeMap<WaitId, OpenWait{kind, armed_at, last_signal_at,
   expected_by}>`, replaced by the Stop payload and backfilled from
   transcript arm/ack pairs and `<task-notification>` closes. Fold gains
   `waiting` (green, own class) between `delegating` and `idle`. Per-kind
   caps: bg Bash `max(2×timeout, 20 min)` hard 1 h; timed Monitor
   `expected_by + 5 min`; persistent Monitor `last_signal + 2 h`; loop/one-shot
   cron `expected_by + 2 min`; recurring cron uncapped.
3. *Children.* Kind from the meta file: teammate iff `taskKind ==
   in_process_teammate` (stem `agent-a<name>-<hex>`, `agentType` holds the
   name), workflow agent iff path under `wf_*/`, else subagent. Launch
   record is the `toolUseResult` object on the parent's ack line
   (`async_launched|forked|teammate_spawned`, `runId`, `workflowName`,
   `transcriptDir`), not a prose prefix. Journal has `launched|started|
   result|failed`; Go ignores `failed` (a live defect). A run ends exactly
   at the lead's `task-notification` line whose `<tool-use-id>` matches.
   Teammate idle vs ended is only distinguishable by `TeammateIdle` /
   `SubagentStop`; an idle team makes the root **idle, not delegating**
   (Go renders it green). Keep `stale_cap` 30 min and `quiet_grace` 90 s
   as last resort only. `~/.claude/sessions/<pid>.json` carries
   `pid, procStart, sessionId, cwd, status` and is a second discovery source.
4. *Blocks.* Every API failure is one shape: an `assistant` line with
   `isApiErrorMessage:true`, `error ∈ rate_limit|server_error|
   authentication_failed|…`, `quotaLimits.resetsAt` (epoch) on limit hits.
   `Stop` does not fire on these; `StopFailure` does (main thread only,
   must be registered). Corpus: no class self-recovers except the explicit
   auto-continue path (`system/informational` notices). `ClaudeRoot` gains
   `block: Block::{None, UntilTime{at, resumes, kind}, Human{reason}}`, set
   only by the transcript tail (64 KiB window, literal-needle prefilter,
   walk back past bookkeeping lines), cleared by a later real line or
   `UserPromptSubmit`, expiring to idle at `at + 60 s`. Fold: permission >
   blocked > working > delegating > waiting > idle. Limits are
   account-wide: hoist `max(resetsAt)` onto every idle Claude root.

Still unmeasured live (the capture harness settles all of these):
`PermissionDenied` on a human decline; `Stop` on Esc; `idle_prompt`
timing; pre-first-token double-Esc; `UserPromptSubmit.source` on a
task-notification; `Stop.background_tasks` listing a persistent monitor;
`SubagentStop` for workflow agents and teammate shutdown; `TeammateIdle`
on error; hook `agent_id` form for teammates; `Notification{quota_auto_resume_fired}`
on 2.1.268; `StopFailure` for `-p` headless roots.

Per-provider reducers are `fn step(root: &mut ClaudeRoot, ev: ProviderEvent, now) -> (Observation, Vec<Effect>)`.
The transition tables in survey 03 §2.3–§2.8 are the spec; the Go tests named in
03 §7 are the acceptance tests.

Navigate seams:

```rust
pub trait WindowManager: Send + Sync {
    fn name(&self) -> &'static str;
    async fn clients(&self) -> Result<Vec<Window>, WmError>;
    async fn active_window(&self) -> Result<Option<WindowRef>, WmError>;
    async fn focus(&self, w: &WindowRef) -> Result<(), WmError>;
    async fn subscribe(&self, cancel: CancellationToken) -> Result<mpsc::Receiver<WmEvent>, WmError>;
}
pub trait Terminal: Send + Sync {
    fn name(&self) -> &'static str;
    async fn snapshot(&self) -> Result<HashMap<Tty, PaneRef>, TermError>;   // the only enumeration
    async fn activate(&self, p: &PaneRef) -> Result<(), TermError>;
}
pub fn join(pane: &PaneRef, clients: &[Window]) -> Option<Window>;         // marker first; 0 or >1 → None
```

Carry wezterm's `is_active` onto `PaneRef` from day one so `focused` can be
pane-level; that removes the "skip every focused row" workaround in the
attention ring (survey 04 §6, issues #91/#92).

Open: whether `SessionState` carries the legacy `claude`/`codex` twin blocks
for v3 compatibility (a custom `Serialize` can emit them from one field), and
whether `Timestamp` is `jiff::Timestamp` or a hand-rolled RFC 3339 newtype.

## 5. Rules that must survive (the short list)

Full registers: survey 07 §2 (decisions D-1..D-36) and §3 (hazards). The
ones a first implementation gets wrong if it does not know them:

1. Discovery is truth; hooks enrich. A hook for an unknown pid creates nothing.
2. Death only from `Gone` or reclassification to non-agent; any other read
   error is "leave alone" (L4c).
3. One writer of `session_end`, four triggers, dedup by map membership (L5/L7).
4. Missed RED is the worst error. A prompt is owned by the writer that raised
   it; only that writer's evidence closes it; red leaves only when every
   writer's set is empty; non-tool hooks never clear a prompt; a `tool_name`
   match from a non-owner never clears; the fanout floor holds main-thread
   shape matches while children are in flight.
5. Hooks are lossy: decline and interrupt fire nothing. The transcript is a
   falsifier of red, never a source of red.
6. Into working: anchor to the transcript entry, floored at the previous
   anchor. Into idle/permission: anchor to wall-clock now (H1/H7/H8). Anchors
   never move backward.
7. Freshness is authority: an expired graph is grey, never a frozen color.
8. Publish only on observable change; clocks are excluded from the change key;
   `fresh_until` is published on a 5 s ceiling so suppression is sound.
9. Hyprland event addresses lack the `0x` prefix its query API has; normalize
   at the seam.
10. Ambiguous window join returns nothing and retries; a miss never clears an
    established mapping.
11. Hydrate may falsify a persisted prompt against the transcript, never
    manufacture one; re-stamp anchors to startup.
12. `SystemTime`/`Instant` (or `jiff::Timestamp`/`Instant`) are distinct on
    purpose; each compile error there is a design question.
13. The pane title is a third signal stream. A double-Esc before the first
    token (H9) fires no hook and writes no transcript line; only the title's
    idle glyph shows it. The Go graph path lost this recovery in August and
    never got it back. The resolver already carries the raw pane title on
    `PaneRef`; phase 4 must feed it to the Claude reducer as positive
    evidence of idleness only, never as a way to clear red.

## 6. Simplifications, itemized

| # | Go today | Proposed | Removes |
|---|---|---|---|
| S1 | 5 binaries, 30 packages | 1 crate, 2 binaries | package glue, 5 copies of `defaultSocketPath` |
| S2 | `RwMutex` store + 3 fence mechanisms + lock order | actor core, inbox order, `seq` compare | `broadcastGen`, `resolveTurn`, per-root generations, persist batching |
| S3 | pid identity + scanner seen-set + `Forget` | `ProcId(pid, starttime)` | seen-set, `Forget`, decisions #12, zombie loop |
| S4 | two Claude status engines + three copies of the pending set | one graph path; one `BTreeMap<Writer, Vec<PendingPrompt>>` | ~1.4k lines of `main.go`/`rpc.go`, `shadow.go`, compat projection |
| S5 | four bespoke timers with four fencing idioms | `Effect::Deadline` + generation | `time.AfterFunc`, `codexTimerWG` |
| S6 | 3–5 tail decodes per writer per tick | one decoded tail per file per tick, cached by `(size, mtime)` | most transcript I/O |
| S7 | `Resolve`/`Reconcile`/`ReconcileFrom`/`findWindow` | `enumerate()` + pure `join()` | three entry points |
| S8 | Codex: app-server graph + hook reducer fused by four overlays | hook reducer only (v1) | 2.4k lines, JSON-RPC client, supervisor |
| S9 | history: eight write sites | history = subscriber of `Effect::History` from the core | scattered `sink.Record` calls |
| S10 | three renderer binaries + legacy bottombar modes | `ctl tui`, `ctl bottombar publish` | polybar, standalone waybar, reaper, process-group kill |
| S11 | `wm=x11`, macOS/Windows stubs, federation | trait seams only | ~6k lines |
| S12 | timeline reader in ctl | Go `switchboard-ctl timeline` stays as the dashboard provider | ~2k lines until v2 |

## 7. The oracle: run both daemons

The Go daemon keeps running until cutover. The Rust daemon uses distinct
paths (`-socket`, `-state`, `-history-dir`) so both can run at once (OQ7 in
the requirements, verified feasible by agent-watcher). With `state.json` v3
kept JSON-equal, the differential test is:

```
jq -S 'del(.updated_at) | .sessions[] |= del(.agent_graph.observed_at, .agent_graph.fresh_until)' go.json
jq -S '…same…' rust.json
diff
```

run on a timer during a working day. That is the cheapest possible
correctness oracle, and it is the main argument for not breaking the wire in
v1. The trace-replay harness the kickoff doc wanted was never built in the Go
repo and the agent-watcher corpus is keyed to the abandoned FSM; do not
resurrect it.

## 8. Build order

Sized so each phase is one sitting to a few, teaches one Rust thing, and
ends with something observable. The owner writes every file under `src/`;
agents port Go test tables, extract fixtures (`hyprctl -j`, `wezterm cli
list`, transcript lines), and review.

| Phase | Builds | Learns | Done when |
|---|---|---|---|
| 0 | `main.rs`: tokio runtime, clap, tracing, `CancellationToken` from SIGTERM/SIGINT, one `interval` | runtime, `select!`, shutdown | daemon logs a tick and exits cleanly on `systemctl stop` |
| 1 | `proc/` + `discovery.rs` + scanner task + `AsyncFd` pidfd watch | `OwnedFd`, `AsyncFd`, traits, pure parsers, table tests | logs "appeared/died" for real Claude/Codex processes; Go `discovery_test.go` tables green |
| 2 | core actor, `SessionState`, `Snapshot`, change key, publisher, RPC `list`/`subscribe`, `ctl list` | `mpsc`/`watch`, `Arc`, serde, actor ownership | `ctl list` matches Go `switchboard-ctl list` for pid/cwd/tty/started_at |
| 3 | `navigate/`: Hyprland client, wezterm, `join`, resolver task, WM events + debounce, `focus-session`, `ctl focus/cycle/attention` | `UnixStream`, `tokio::process`, parsing, pure policy | clicking a chip focuses the right pane; differential diff clean on `wezterm`/`hyprland` blocks |
| 4 | `graph/`, hook socket ingest + ppid attribution, Claude prompt machine + transcript tails + fanout scan, Codex hook reducer, deadlines | enums as state machines, `BTreeMap`, `Option` discipline, borrow-checker-shaped design | Go `TestReduceTruthTable` + the 03 §7 invariant tests green; a real `PermissionRequest` goes red and clears; diff clean on `claude`/`codex`/`agent_graph` |
| 5 | history sink, `ctl bottombar publish`, waybar chip render, `ctl tui`, hydrate | blocking-thread bridging, atomic files, signals | waybar and dashboard run against the Rust daemon; Go unit disabled |

Phase 4 is the hard one and the one the surveys document best. Phases 1–3
are where async Rust is learned; the status machine in phase 4 is mostly
pure code.

## 9. Salvage from agent-watcher (lift, do not import)

From survey 08 §6, the pieces worth copying with light edits, roughly 1,500
lines: `aw-proc/src/linux/stat.rs` (stat parser using the last `)`),
`procfs.rs` errno classification, the pidfd `open`/`waitid` wrappers,
`aw-discovery` match-rule data, `aw-wm/src/hyprland.rs` parsers plus real
`hyprctl -j` fixtures and both focus dialects, `aw-terminal/src/wezterm.rs`
gui-sock discovery and list fixture, `aw-agent/src/claude.rs` transcript notes
and torn-tail reader, `hook_socket.rs` bind (0600/0700, stale reclaim),
`aw-timeline` if the dashboard envelope is ever ported. Read for ideas: the
`!Send` state-lock guard, the biased `select!` tick, the two-timestamp trace
format. Ignore the rest.

## 10. Decisions to make before phase 2

1. `state.json`: keep v3 JSON-equal (recommended, enables §7) or design v4 now.
2. Codex in v1: hook-only (recommended), with app-server observer, or not at all.
3. Dashboard: Go `timeline` stays as provider (recommended) or port in v1.
4. Federation: out of v1 (recommended); keep `(hostname, pid, started_at)` and full-replacement frames as constraints.
5. Bars: bottombar attached publisher + waybar chips only (recommended); polybar dropped.
6. Core: actor (recommended) or mutex store.
7. WM breadth: Hyprland only in v1; sway/i3 via `swayipc-async` as phase 3.5 if wanted.
8. Time type: `jiff` (recommended) or hand-rolled RFC 3339 newtype.
9. Where the owner wants agents: test-table ports and fixtures only (recommended), or also first drafts of leaf modules.
