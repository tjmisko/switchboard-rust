# Types, and the transitions that are actually hard

Companion to `02-session-machine.md`. That doc settled the *shape* (phase as a
fold, a turn holding a tool map, children sharing the activity machine). This
one asks the narrower question: for each transition the reducer must make,
what does the harness give us, and what is left to inference?

The short version: three channels found on 2026-09-11 collapse most of the
"hard" list. Two of them are Claude-only, and that asymmetry is now the main
architectural fact about the Codex provider.

## 1. Channels beyond hooks

### 1.1 `statusLine` — a heartbeat with a payload (Claude)

```
statusLine: { type: "command", command: string, padding?: number,
              refreshInterval?: number,   // "Re-run ... every N seconds in
                                          //  addition to event-driven updates"
              hideVimModeIndicator?: bool }
```

The command is spawned with the render payload on stdin (and the same payload
flattened into its environment, plus `CLAUDE_PROJECT_DIR`). Keys verified in
the bundle:

```
version, output_style{name}, model, workspace,
cost{ total_cost_usd, total_duration_ms, total_api_duration_ms,
      total_lines_added, total_lines_removed },
context_window, exceeds_200k_tokens, fast_mode, effort{level},
thinking{enabled}, vim{mode}, agent{name}, remote{...},
rate_limits{ status: "allowed"|"allowed_warning"|"rejected",
             resetsAt: int, rateLimitType, utilization,
             unifiedWindows{ five_hour{utilization, resetsAt},
                             seven_day{...}, seven_day_opus{...},
                             seven_day_sonnet{...} } }
```

Two things here are worth more than the whole transcript tail:

- **`rate_limits`** is the `Block` axis, delivered as structured data on every
  render. `survey/13` designed a 64 KiB tail scan with a literal-needle
  prefilter to recover `quotaLimits.resetsAt` from an error line. That scan
  becomes a fallback for sessions whose statusline is not wired.
- **`cost.total_api_duration_ms`** is a direct measurement of whether the
  model is being called. Not "has the transcript grown", not "is there CPU" —
  the harness's own accumulator for time spent in the API. Two consecutive
  refreshes with no delta means no request is in flight.

### 1.2 `subagentStatusLine` — the child table (Claude)

```
subagentStatusLine: { type: "command", command: string }
  // "Custom per-subagent status line shown in the agent panel;
  //  receives row context as JSON on stdin"
```

Payload is the whole panel, not one row:

```
{ ...base, columns,
  tasks: [ { id, name, type, status, description, label, startTime,
             model, effort, contextWindowSize, tokenCount,
             tokenSamples, cwd } ] }

status ∈ pending | running | completed | failed | killed | paused
```

`failed` and `killed` are the two terminal states no hook emits and that
`02-session-machine.md` §4.2 called the per-child leak generator. The harness
tracks them and will hand them over. This single channel retires the
mandatory-expiry design, the capacity bound, and the LRU eviction — see §3.

### 1.3 What Codex has instead: not much

- `legacy_notify` fires `agent-turn-complete{thread-id, turn-id, cwd, client,
  input-messages, last-assistant-message}`. The symbol is literally named
  `legacy_notify`; it is superseded by hooks and adds nothing.
- `ThreadBackgroundTerminal{ itemId, processId, osPid, cpuPercent, rssKb }`
  exists, but over the **app-server**, which is deferred in v1. It is the
  Codex analogue of `Stop.background_tasks` and the reason to bring the
  app-server forward.
- No statusline. No child table. No per-render payload of any kind.

So Codex keeps its API-error blind spot with no harness-side instrument, and
the honest conclusion is in §4.2.

### 1.4 `CLAUDE_SESSION_ID`

Exists as an exported environment variable. If it is present in the agent
process's own `environ` (unverified — a cheap, high-payoff measurement), then
`/proc/<pid>/environ` yields the session id **at discovery**, closing the
window where a live session is known by pid but unusable until its first hook
arrives. Worth one experiment before the discovery code is written.

## 2. The types

```rust
// ---------- identity ----------
struct RootKey  { pid: Pid, start: StartTime }   // reuse-proof, from one stat read
struct SessionId(Uuid);                          // rotates on /clear
struct AgentId(Box<str>);                        // normalised at the adapter
struct ChildKey { session: SessionId, agent: AgentId }
struct ToolUseId(Box<str>);
struct InputHash([u8; 8]);                       // canonical-JSON sha256, truncated

// ---------- the turn, shared by roots and children ----------
enum Activity {
    Idle { since: Instant },
    Turn { started: Instant, tools: BTreeMap<ToolUseId, Tool>, last_edge: Instant },
}

struct Tool { name: ToolName, hash: InputHash, since: Instant, gate: Gate }

enum Gate {
    Open,                                  // PreToolUse seen, running
    Blocked { asked_at: Instant },         // PermissionRequest joined to this call
}
```

The important move: **a permission prompt is a state of a tool call, not a
separate record set.** The Go daemon keeps `pending: BTreeMap<Writer,
Vec<PendingPrompt>>` with a latch because `PermissionRequest` carries no
`tool_use_id`. With `PreToolUse` wired unrestricted, the prompt attaches to
the call it belongs to and the latch disappears. The join is §4.1.

```rust
// ---------- the root ----------
struct Root {
    key:        RootKey,
    session:    SessionId,
    provider:   Provider,
    life:       Life,                      // pidfd-backed: Alive | Gone(at)
    place:      Placement,                 // tty, pane, window, focus
    activity:   Activity,
    orphans:    Vec<OrphanPrompt>,         // PermissionRequest that would not join
    limits:     Limits,                    // from statusline rate_limits
    block:      Block,
    waits:      BTreeMap<WaitId, OpenWait>,
    compacting: bool,
    children:   Children,
    beat:       Option<Beat>,              // last statusline refresh
}

struct Beat { at: Instant, api_ms: u64, ctx: u32, model: ModelId }

enum Block { None, UntilTime { at: SystemTime, kind: LimitKind }, Human { reason: Reason } }

// ---------- children: a snapshot, not an accumulator ----------
struct Children { rows: BTreeMap<AgentId, ChildRow>, refreshed: Option<Instant> }

struct ChildRow {
    kind:     ChildKind,                   // Subagent | Teammate | WorkflowAgent
    status:   ChildStatus,                 // authoritative, from the panel
    started:  SystemTime,
    label:    Box<str>,
    tokens:   u64,
    activity: Activity,                    // refined between refreshes by hooks
}

enum ChildStatus { Pending, Running, Completed, Failed, Killed, Paused }
```

`Phase` is not a field anywhere. It is
`fn phase(&Root) -> Phase` folding permission > blocked > working >
delegating > waiting > idle, exactly as in `02` §2.

## 3. The design principle the child table forces

**Roots are edge-triggered; children are level-triggered.**

A root is a process: it has a pidfd, discrete events, and an exact death
signal, so it is driven by edges and the map of roots is owned by discovery.
A child is a row in someone else's table: it has no pid and no death signal,
but the table is republished in full on every refresh. So `Children.rows` is
*replaced*, never merged. Absence from the newest table is a fact, not a
timeout, which means:

- no mandatory per-child expiry,
- no capacity bound or eviction policy,
- no 30-minute stale cap,
- no "did the journal say failed" parsing (`survey/11` found the Go code
  ignores the `failed` line; the table reports `failed` directly),

all of which `02-session-machine.md` §4.2 listed as the cost of per-child
machines. Hooks still refine a row between refreshes — `SubagentStart` and
the child's own tool events give sub-refresh resolution for `activity` — but
they may never *create* a row and never *remove* one. Same shape as the
existing "discovery is truth, hooks only enrich" rule, one level down.

Fallback when `subagentStatusLine` is not configured: the old `subagents/`
scan, with the expiry machinery, behind a feature flag. It is the degraded
path, not the design.

## 4. The transitions, ranked by how hard they actually are

### 4.1 `Tool::Open -> Tool::Blocked` — join a prompt to a call

`PermissionRequest` carries `tool_name` and `tool_input` but **no
`tool_use_id`**; this did not change in 2.1.268 and is the one gap the richer
hook set left open. Join on `(writer, tool_name, hash(tool_input))` against
the open tools of that writer.

Sound because `PreToolUse` and `PermissionRequest` both observe the
*pre-approval* input. The rewritten input appears only on `PostToolUse`
(`updatedInput`), so `PostToolUse` must be matched **by id, never by hash** —
a rule the types should enforce by not giving `PostToolUse` a hash field at
all.

Residue: two parallel calls to the same tool with byte-identical input.
Ambiguous and harmless — pick either; they are indistinguishable to the user
too. Real residue: a `PermissionRequest` arriving when the daemon started
mid-turn and never saw the `PreToolUse`. That goes to `orphans` and is the
only prompt that needs a timeout.

### 4.2 `Turn -> Idle` with no terminal event

Two causes, one per provider, and they are not equally tractable.

**Claude, interrupt during generation with no tool open — measured, and not
actually blind.** Interrupt *with* a tool open is exact
(`PostToolUseFailure{tool_use_id, is_interrupt}`). For the no-tool case, the
corpus settles it: 69 real interrupts across `~/.claude/projects`, of which
**13 are confirmed interrupts of a pure-generation message** (the interrupted
assistant message contained no `tool_use` block), 18 had a tool open, and 38
targeted a message that had rolled out of the file. The transcript records
every one of them as a *positive* line, immediately:

```json
{"type":"user","parentUuid":"cf6b1c83-…","promptId":"649a5424-…",
 "message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]},
 "uuid":"0f5dcd32-…","timestamp":"2026-09-11T01:35:09.339Z",
 "interruptedMessageId":"msg_011CevnxPftc6xkTQUsbmWYo",
 "session_id":"74f5b0cf-…","cwd":"…","userType":"external","entrypoint":"cli"}
```

Two consequences for the types:

1. **Key on `interruptedMessageId`, never on the marker text.** It is a
   top-level field carrying the exact `message.id` that was killed, so the
   edge joins to the assistant message rather than being matched by prose.
   There are also *two* marker strings in the corpus —
   `[Request interrupted by user]` (63) and
   `[Request interrupted by user for tool use]` (6) — so any reducer matching
   one literal silently misses the other. A top-level-field check has no such
   failure mode. Corroborating signal: 15 of the interrupted assistant
   messages carry `stop_reason: null`, i.e. the message was never completed.
2. **This is a latency problem, not an observability problem.** Lines land
   ~50 ms after their own timestamp (`survey/10`), so the only thing standing
   between the daemon and an exact interrupt edge is the 5 s transcript poll.
   `inotify` on the transcript, or the statusline beat used as a read
   trigger, closes it to well under a second.

So `cost.total_api_duration_ms` is demoted from mechanism to corroboration,
which is the right place for it: it rested on an unverified assumption about
when the counter advances, and nothing now depends on that.

**Codex, a turn that dies on an API error — the app-server has it exactly.**
No hook, no statusline, no non-legacy notify. But the app-server protocol,
generated from the binary with
`codex app-server generate-json-schema --out DIR`, types it directly:

```
TurnStatus  = completed | interrupted | failed | inProgress
Turn        { id, status, error: TurnError|null, startedAt, completedAt, durationMs, items }
TurnError   { message, additionalDetails?, codexErrorInfo?, misalignment? }
CodexErrorInfo = httpConnectionFailed | responseStreamConnectionFailed
               | responseStreamDisconnected | responseTooManyFailedAttempts
               | activeTurnNotSteerable
```

`turn/completed` carries both exits the hooks cannot see — `failed` with a
typed error, and `interrupted`. The app-server also carries
`account/rateLimits/updated{RateLimitSnapshot{primary, secondary:
RateLimitWindow{usedPercent, resetsAt, windowDurationMins},
rateLimitReachedType, planType, credits}}`, which is the `Block` axis for
Codex — the exact counterpart of Claude's statusline `rate_limits`, and a
channel Codex otherwise does not have at all.

**The catch, measured.** A separately-spawned app-server reports the running
TUI's threads as `status: {"type": "notLoaded"}` and sends no live
notifications for them; `thread/loaded/list` returned `[]` while two Codex
TUIs were running. The client-request list contains `thread/unsubscribe` and
**no `thread/subscribe`** — subscription is a side effect of `thread/start`,
`thread/resume` and `thread/fork`. So this data is available for threads the
daemon *owns*, and not for the ones a human starts by typing `codex`.

That is a fork in the architecture, not a tuning parameter:

- **(a) The daemon launches Codex sessions** via `thread/start`, making the
  TUI a client of the daemon rather than a sibling. Everything above becomes
  exact. Large change in how the user starts a session.
- **(b) Human-launched sessions keep the hook path** and its guesses, and the
  app-server is used only for what it can see from disk — `thread/list` gives
  existence, `cwd`, `gitInfo{sha, branch, originUrl}`, a real `name`,
  `parentThreadId`, `model` and `reasoningEffort` without touching `/proc`.
- **(c) `remoteControl`** announced itself unprompted on the very first
  capture (`{"method":"remoteControl/status/changed","params":{"status":
  "disabled","serverName":"goosebook",…}}`) and has not been examined. If it
  is a sanctioned way to observe an existing session, it is the answer.

Until (a) or (c) is decided, the fallback for a human-launched Codex turn
that dies on an API error is the §5 staleness rule with a long window.

### 4.3 Async and background children — completion is not `PostToolUse`

For a synchronous `Agent` call the parent's `PostToolUse` *is* the child's
death certificate, with an exact id. For `async_launched`, `forked` and
`teammate_spawned`, `PostToolUse` is a launch ack (`survey/11`), and
completion arrives later as a user turn with `origin.kind:
task-notification` whose prompt text carries the originating
`<tool-use-id>`.

`UserPromptSubmit` fires for that turn and carries `prompt`. So the forwarder
extracts the `<tool-use-id>` and forwards **only the id**, never the prompt —
the privacy boundary holds and the close edge becomes exact. The child table
covers the same fact at refresh granularity; the hook gives it immediately.

### 4.4 Identity rotation on `/clear`

`SessionStart{source: "clear"}` on a live pid: same `RootKey`, new
`SessionId`. Hard because in-flight events for the old id must be rejected
and every child must be dropped. `ChildKey` embedding `SessionId` makes the
second automatic — the old rows are unreachable rather than deleted — and the
first is a single equality check in the adapter.

### 4.5 `Block` — no longer hard

`rate_limits.status == "rejected"` with `resetsAt` gives
`Block::UntilTime` exactly, per render, with the window type. The account-wide
hoist (`survey/13`: one session's limit applies to all) becomes correct
rather than approximate, because every statusline-wired session reports the
same `unifiedWindows` independently. The transcript tail scan survives only
for sessions without a statusline.

### 4.6 Root liveness — already solved, but discovery has a window

`pidfd` is exact. The residue is the gap between discovering a pid and
learning its `SessionId` from the first hook. If `CLAUDE_SESSION_ID` is in
the process's own environ, `/proc/<pid>/environ` closes it at discovery
(§1.4).

## 5. Integration constraint: the statusline is already taken

There is one `statusLine` command per settings scope, and this machine
already has `~/.claude/claude-status`. Switchboard cannot own it.

The forwarder must **tee**: read stdin to EOF, send the parsed subset to the
daemon over the socket, then spawn the user's real script with the same bytes
on its stdin and pass its stdout through unchanged. Configuration becomes
`"command": "switchboard-ctl statusline -- ~/.claude/claude-status"`.

Consequences that belong in the design, not in a README:

- The marginal process cost is zero. The user already pays a spawn per
  render; the tee adds a socket write to a spawn that was happening anyway.
- The forwarder is now on the *rendering* path. A slow or hung ctl stalls the
  status line. It must have no runtime, a hard write deadline, and must exec
  the wrapped script even when the daemon socket is dead.
- The daemon must debounce: event-driven refreshes can arrive several times a
  second, and only `rate_limits`, `api_ms` and `model` matter. Compare
  against `Root.beat` and drop unchanged ones before they reach the reducer.
- Same shape for `subagentStatusLine`, which is currently unset — so there
  switchboard can own it outright, with a note that it must degrade to a
  pass-through if the user later wants their own.

## 6. What to measure, in priority order

1. Does `cost.total_api_duration_ms` advance *during* a turn, or only at turn
   end? §4.2 depends on it entirely.
2. Is `CLAUDE_SESSION_ID` present in the agent process's own `/proc/<pid>/environ`?
3. How often is `statusLine` invoked without `refreshInterval` — per render,
   per token, or debounced by the TUI? Determines the debounce budget.
4. Is `subagentStatusLine` invoked when the agent panel is closed, or only
   while it is visible? If only visible, the child table is not a reliable
   level signal and the `subagents/` fallback is not a fallback.
5. What is `tasks[].type`, and does it distinguish teammate from workflow
   agent from plain subagent? If yes, `survey/11`'s meta-file kind rules are
   redundant.
6. Does a `paused` child mean the teammate-idle state, closing the
   `TeammateIdle` gap in `survey/11` without a hook?
7. Element shapes inside `Stop.background_tasks[]` and `session_crons[]`.
8. `CLAUDE_ASYNC_AGENT_STALL_TIMEOUT_MS` — the harness's own stall notion for
   async agents. Read it rather than inventing a cap.

Items 1, 3 and 4 are answered by the same recording as the hook capture
harness, with the tee installed for a working day.

---

# Part II — the type design (2026-09-11)

## 7. Organising principle: edges and levels, not sources

Every input is one of two epistemic kinds, and the merge rule differs, so
they are different types.

| Input | Scope | Kind |
|---|---|---|
| Claude/Codex hooks | session, call, child | edge |
| transcript lines | session, call, turn | edge (delayed ~50 ms) |
| app-server notifications | thread, turn, item | edge |
| pidfd readable | process | edge |
| Hyprland event socket | window | edge |
| `statusLine` | session + account | level |
| `subagentStatusLine` | children | level |
| `subagents/` scan | children | level |
| `/proc` sweep | process | level |
| `hyprctl clients` / `wezterm cli list` | surfaces | level |
| app-server `thread/list` | threads | level |

Edges are exact, lossy, and must be idempotent by id. Levels are
self-correcting, may be stale, and replace a whole scope.

**The rule that makes them safe to mix:** a level never overrides an edge
observed after the level was captured. Without it a stale panel snapshot
resurrects a child that `SubagentStop` just closed.

```rust
struct Leveled<T> { value: T, captured: Instant }
// apply iff captured > scope.last_edge
```

## 8. Scope the state to the thing it describes

`Block` does not belong on a session. Usage limits are account-wide; putting
them on each root and hoisting `max(resetsAt)` is a workaround for a
misplaced field.

```rust
struct World {
    accounts:  BTreeMap<AccountId, Account>,   // limits, plan
    processes: BTreeMap<ProcKey,   Process>,   // pid, pidfd, cwd, tty, cpu
    sessions:  BTreeMap<SessionId, Session>,
    surfaces:  BTreeMap<PaneKey,   Surface>,   // pane, window, focus
}
```

`BTreeMap` throughout, because the publish gate is a change key computed over
the serialised snapshot. `HashMap` iteration order would produce spurious
publishes.

## 9. Identity

```rust
struct ProcKey { pid: Pid, start: StartTime }  // one /proc/<pid>/stat read, reuse-proof
struct SessionId(Uuid);    // rotates on /clear
struct PromptId(Uuid);     // Claude turn id: hook base prompt_id == transcript promptId
struct TurnId(Box<str>);   // Codex
struct ToolUseId(Box<str>);
struct AgentId(Box<str>);  // normalised at the adapter boundary
struct RequestId(Box<str>);// app-server server-request id; string or int on the wire
```

`prompt_id` is the join key between Claude's hook stream and its transcript.
Both turn identifiers make turn edges idempotent.

## 10. Two clocks

```rust
struct Observed { local: Instant, claimed: Option<SystemTime> }
```

`local` drives every timeout and every ordering decision. `claimed` is for
display and for foreign deadlines (`resetsAt`). Transcript timestamps are
*generation* time and can precede the hook reporting the same fact, so
ordering across sources by wall clock is wrong.

## 11. Edges

```rust
enum EdgeKind {
    SessionStarted { source: StartSource, title: Option<Box<str>> },
    SessionEnded   { reason: Box<str> },
    TurnStarted    { turn: TurnKey, origin: Origin },
    TurnEnded      { turn: TurnKey, outcome: TurnOutcome },
    ToolDispatched { call: ToolUseId, tool: ToolName, hash: InputHash },
    ToolSettled    { call: ToolUseId, outcome: ToolOutcome },
    PermissionAsked   { call: Option<ToolUseId>, tool: ToolName,
                        hash: Option<InputHash>, owner: Ownership },
    PermissionSettled { call: Option<ToolUseId>, req: Option<RequestId>, outcome: Decision },
    ChildStarted   { agent: AgentId, kind: ChildKind },
    ChildEnded     { agent: AgentId, outcome: ChildOutcome },
    CompactionStarted { trigger: Trigger },
    CompactionEnded,
    CwdChanged { to: PathBuf },
    Died,
}

enum TurnOutcome {
    Completed,
    Interrupted { killed: Option<MessageId> },   // transcript interruptedMessageId
    Failed { class: ErrorClass },                // StopFailure | Codex TurnError
}
enum ToolOutcome { Ok { ms: Option<u32> }, Failed { interrupt: bool }, Denied { by: Denier } }
enum Denier { User, PermissionRule, AutoMode, Unavailable, Interrupted }
enum Ownership { Human, Machine, Unknown }
```

`Denier` is the measured `toolDenialKind` vocabulary (permission-rule 274,
automode-blocked 71, user-rejected 21, automode-unavailable 4, interrupted 1).
Only `User` is a human decision; the others are policy.

## 12. Session state

```rust
struct Session {
    id:         SessionId,
    provider:   Provider,
    proc:       ProcKey,
    activity:   Activity,
    prompts:    Prompts,
    compaction: Compaction,
    waits:      BTreeMap<WaitId, Wait>,
    children:   Leveled<BTreeMap<AgentId, Child>>,
    beat:       Option<Leveled<Beat>>,
    title:      Option<Box<str>>,
}

enum Activity {
    Idle { since: Instant },
    Turn { key: Option<TurnKey>, started: Instant,
           tools: BTreeMap<ToolUseId, Tool>, last_edge: Instant },
}

struct Tool { name: ToolName, hash: InputHash, since: Instant, gate: Gate }
enum Gate { Running, Blocked { since: Instant, owner: Ownership, req: Option<RequestId> } }

struct Prompts { unjoined: Vec<Unjoined> }
struct Unjoined { tool: ToolName, hash: Option<InputHash>,
                  since: Instant, owner: Ownership, writer: Writer }
```

No `Thinking` variant: it is `tools.is_empty()`. No `blocked: bool`: a block
is a state of the call that caused it.

Absence is a state, never `Option`, wherever it renders:

```rust
enum Placement { Unknown, Pane(PaneKey), Unnavigable }
enum Compaction { None, InProgress { since: Instant, trigger: Trigger } }
```

## 13. Six transitions that need care

1. **Permission join with no id.** Claude `PermissionRequest` carries no
   `tool_use_id`. Join on `(writer, tool_name, hash(tool_input))` against the
   writer's open calls. Hooks are separate processes, so arrival order is not
   guaranteed: join must be attempted from both directions, with `unjoined`
   holding a prompt that arrived before its `PreToolUse`. Match
   `PostToolUse` by id only — its `tool_input` is the post-approval rewrite.
2. **Level/edge race.** §7's `captured > last_edge` rule.
3. **Turn end with no terminal edge.** Staleness may close a turn. It may
   never clear attention or a block: a session parked on a prompt has no CPU
   and no transcript growth and looks exactly like a dead turn.
4. **Identity rotation.** `SessionStart{source: "clear"}` on a live pid keeps
   `ProcKey` and replaces `SessionId`. Children keyed by `SessionId` become
   unreachable rather than needing deletion; events carrying the old id are
   rejected by one equality check in the adapter.
5. **Ownership.** Never render attention without evidence. `Ownership::Human`
   renders; `Machine` never does; `Unknown` renders as unknown after a grace.
   This is the whole Codex `approvalsReviewer` question, encoded so the fold
   cannot get it wrong.
6. **Writer-blind sweeps.** Codex does not guarantee `agent_id` on child
   hooks, so a root `Stop` may sweep children's *activity* and must not touch
   their prompts.

## 14. Deliberately not used

- **Typestate / session types.** The states are runtime data driven by
  untrusted input; encoding them in the type parameter buys nothing and makes
  the map of sessions unrepresentable.
- **A provider trait with dynamic dispatch.** Adapters are plain
  `fn(RawEvent) -> Option<Input>`. Two of them.
- **`async` in the reducer.** `fn step(&mut World, Input) -> Vec<Effect>` stays
  pure and synchronous; I/O lives in the effect runner.

## 15. Testing ground truth

The OTel channel exports a span named `claude_code.tool.blocked_on_user`,
alongside `claude_code.tool.decision`, `claude_code.subagent.spawn`,
`claude_code.compaction` and `claude_code.active_time.total`. Spans export on
end, so this is useless as a live signal, but a trace recorded next to a hook
capture is an independent record of when the harness itself considered a
session blocked — the natural acceptance test for the reducer.
