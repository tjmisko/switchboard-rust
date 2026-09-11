# Survey 01 — Daemon core (`cmd/switchboard`, `internal/state`, `internal/rpc`, `buildinfo`, `durfmt`, `detect`)

Read-only survey of the Go daemon at `~/Projects/switchboard` as of 2026-09-10, for the Rust rewrite.
All paths are relative to that repo. Line numbers are from the files as read; they drift, the
symbol names do not. `codex_hook_transitions.go` and `agent_observation.go` are covered by another
survey; here they appear only where `main.go` calls into them.

## 1. Responsibilities

- **Own the process**: parse flags, log the build revision, install SIGINT/SIGTERM as context
  cancellation, wire every subsystem, block until the RPC server exits, then tear down in
  reverse (`cmd/switchboard/main.go:83-303`).
- **Own the session map**: `state.Store` is the single mutable truth for `map[pid]*Session`;
  every mutation goes through `Store.Apply` (`internal/state/state.go:832`).
- **Publish state**: on every Apply that changes something observable, encode one JSON frame,
  fan it out to socket subscribers, and atomically rewrite `state.json` (`state.go:1191-1362`).
- **Suppress no-op publishes**: a JSON change key gates publication so an idle reconcile tick
  does not wake every bar (`state.go:974`); publish/suppress counts are logged once a minute
  (`internal/state/publishstats.go`).
- **Discover and retire sessions**: react to `/proc` scan discoveries, pidfd deaths, WM
  window-closed events, a startup stale-drop, and a per-tick liveness sweep, all funnelled
  through one `endSession` writer (`main.go:200-244, 320-332, 397-445, 990-1001`).
- **Reconcile mappings**: every `reconcile-interval` (5 s) and after coalesced WM layout events,
  re-resolve each session's terminal pane and WM window, refresh focus, suspension, and the
  legacy Claude status self-heals (`main.go:640-1037, 1180-1553`).
- **Serve the RPC socket**: JSONL over a Unix socket for bars, `switchboard-ctl`, hooks, the TUI,
  WezTerm Lua callbacks and remote daemons (`internal/rpc/rpc.go`).
- **Attribute hooks**: map a hook subprocess's pid to the tracked agent ancestor and hand the
  hook to the provider coordinator without holding the store lock (`rpc.go:1005-1017, 1407`).
- **Persist and hydrate**: load `state.json` at boot, rebuild prompt ownership from the mirror,
  falsify it against transcripts, re-stamp in-memory clocks to startup (`state.go:1378`,
  `main.go:397-610`).
- **Advertise capabilities**: detect WM/terminal backends once at boot and re-publish the
  capabilities block every tick so a late-starting terminal flips `navigate` (`internal/detect`,
  `main.go:918`).

## 2. Core types

### 2.1 Domain types (`internal/state/state.go`)

| Type | Lines | Notes |
|---|---|---|
| `Session` | 17-96 | Root session row. Fields: `PID`, `Hostname` (aggregate-only), `CWD`, `TTY`, `StartedAt`, `Focused`, `Remote`, `Suspended`, `Headless`, `Navigable`, `LocalWorkspace`, `Agent` ("claude"/"codex"), `DisplayName *DisplayName`, `ResolvedName`, `Wezterm *WeztermInfo`, `Hyprland *HyprlandInfo`, `Claude *AgentInfo`, `Codex *AgentInfo`, `AgentGraph *AgentGraph`. Exactly one of Claude/Codex is populated, chosen by `Agent`. |
| `Session.Enrichment()` / `AgentBlock(kind)` | 122-153 | Read the populated block; allocate it on first hook. |
| `WeztermInfo` | 155-176 | `MuxPID, MuxSocket, PaneID, TabID, WindowID, WindowTitle` on the wire; `Title, TitleAt` are `json:"-"` (raw pane title and its sample time; the H9 idle-title signal). |
| `HyprlandInfo` | 178-183 | `Address, Workspace, WorkspaceID, Monitor` (Monitor always ""). |
| `AgentInfo` (alias `ClaudeInfo`) | 188-325 | `SessionID, Transcript, Status, StatusSinceWire *time (derived), InFlightSubagents, Workflows []WorkflowStatus`; in-memory only: `StatusSince time`, `Pending map[writer]PendingPrompt`, `PendingTool`; wire projections `PendingWriters []string` (derived, sorted, "main" for ""), `PendingPrompts []PendingPromptRecord` (real state, pruned to Pending's key set at snapshot). |
| `PendingPrompt` | 358-362 | `Tool, InputHash, Since` — in-memory correlators. |
| `PendingPromptRecord` | 393-400 | `Writer, Tool, InputHash, CallID, Attention, Since` — the persisted per-call record. |
| `WorkflowStatus` | 335-341 | `RunID, Name, AgentsStarted, AgentsDone, InFlight`; must stay sorted by RunID. |
| Status constants | 112-117 | `working`, `idle`, `permission`, `delegating`; "unknown" is never stored. |
| `Snapshot` | 675-680 | `SchemaVersion, Sessions []Session, UpdatedAt, Capabilities *Capabilities`. |
| `Capabilities` | 687-692 | `Observe, Navigate, WM, Terminal`. |
| `DisplayName` | `display_name.go:24-29` | `Value, Origin (generated|fallback), ConversationID, NativeBaseline *string`. `CurrentSchemaVersion = 3` at `display_name.go:8`. |
| `AgentGraph` | `agent_graph.go:34-48` | `RootID, Source, ObservedAt, FreshUntil, Complete, Summary, Nodes []AgentNode`; unexported `provider`. `MarshalJSON` (59-64) ceilings `FreshUntil` to a 5 s bucket (`FreshnessBucket`, line 14). |
| `AgentGraphSummary` | 70-80 | `Runtime, Attention, Status, LiveChildren, WaitingNodes, ApprovalNodes, UserInputNodes, ErrorNodes, Since`. |
| `AgentNode` | 85-98 | `ID, ParentID, Nickname, Role, Description, Runtime, Attention, Lifecycle, StartedAt, UpdatedAt, CompletedAt, Usage AgentUsage`. |
| `AgentUsage` | 102-110 | seven `int64` token counters, all omitempty; `IsZero` for `omitzero`. |

Enum vocabularies come from `internal/agentgraph/types.go`: `ProviderKind` ("", claude, codex),
`RuntimeState` (unknown, not_loaded, idle, active, system_error), `AttentionState` (none,
approval, user_input), `LifecycleState` (unknown, pending, running, completed, interrupted,
errored, shutdown, not_found), `SourceKind` ("", codex_app_server, hook, claude_transcript,
codex_rollout, restored_last_known).

### 2.2 Plumbing types

| Type | Lines | Notes |
|---|---|---|
| `state.Store` | `state.go:728-775` | `path`, `mu RWMutex`, `sessions`, `subscribers map[chan Broadcast]struct{}`, `caps`, `publishedKey []byte`, `publishedGen`, `broadcastMu`, `broadcastGen`, `frameMu`, `lastBroadcast *Broadcast`, `statsMu`, `stats`, `persistSnapshot func` (test seam), `persistMu`, `persistRunning`, `persistPending *persistBatch`, `afterBroadcastUnlock` (test seam). |
| `state.Broadcast` | 698-711 | `Snapshot` plus shared compact `JSON []byte` (nil on encode failure). |
| `state.PublishStats` | `publishstats.go:55-65` | one closed window of counters + heap/VmHWM readings. |
| `rpc.Request` | `rpc.go:48-149` | one struct for every command; see §4.4. |
| `rpc.Response` | 166-171 | `Snapshot *Snapshot, Diagnostics []AgentDiagnostic, OK bool, Error string`. |
| `rpc.Server` | 231-256 | store, socketPath, `term terminal.Locator`, `wm wm.Manager`, `tun`, `hist`, `fanout` (nil in prod), `agentHook`, `diagnostics`, `view SnapshotView`, `exactFocus`, `paneBind`, `paneState`, `announce`, `hookAttributionDiagnostic`, `readProc` seam. |
| `rpc.SnapshotView` | 220-224 | `Snapshot()`, `CurrentFrame()`, `Subscribe() (<-chan struct{}, func())` — the federated aggregate view. |
| `rpc.Client` | 1484-1506 | `Dial/Send/Recv/Close/SetDeadline`; used by every Go client. |
| `resolveTurn` | `main.go:857-869` | a mutex serialising the two window-resolve producers. |
| `reconcileState` | `fanout.go:23-28` | cross-tick cursors: `fanout *Observer` (nil in prod), `usageOffset map[pid]int64`, `labels map[key]labelCursor`, `names *label.NameCache`. |
| `federationRuntime` | `federation.go:32-42` | hostname, store, remotestate manager, panebind registry, view, navigator, workspace index, announcer, `wg`. |
| `detect.Stack` / `Options` | `detect.go:22-34` | `OSProc osproc.Source, Terminal terminal.Locator, WM wm.Manager`. |
| `buildinfo.Info` | `buildinfo.go:36-52` | `Version` (ldflags), `Revision` (vcs stamp, 7 chars), `Modified` (advisory only). |
| `durfmt` | `durfmt.go`, `clock.go` | `Compact`, `Coarse`, `Since`, `CoarseSince`, `NextCompactChange`, `NextCoarseChange`, `Clock`/`Timer` interfaces. Pure; renderer-side. |

### 2.3 `state.json` schema (frozen, v3)

Pinned by `internal/state/testdata/state.golden.json` and `docs/state-schema.md`. Written with
`encoding/json`, two-space indent, trailing newline; broadcast frames are the compact encoding
of the same value. Field order is struct order.

```jsonc
{
  "schema_version": 3,                     // omitempty; Load ignores mismatched versions
  "sessions": [ /* Session[] sorted by workspace_id asc (unresolved last), started_at, pid */ ],
  "updated_at": "RFC3339",                 // advisory; excluded from the publish gate
  "capabilities": {                        // omitted entirely until SetCapabilities
    "observe": true, "navigate": bool, "wm": "hyprland|sway|i3|x11|none", "terminal": "wezterm|tmux|tmux+wezterm|none"
  }
}
```

Root session (`Session`):

```jsonc
{
  "pid": 4821, "hostname": "…"(aggregate only), "cwd": "…", "tty": "/dev/pts/3",
  "started_at": "RFC3339", "focused": false,
  "remote": true, "suspended": true, "headless": true, "navigable": true,   // all omitted when false
  "local_workspace": 4,                    // aggregate-only, remote rows only, 0 = omitted
  "agent": "claude|codex",                 // omitted until known
  "display_name": { "value": "api-cleanup", "origin": "generated|fallback",
                    "conversation_id": "…", "native_baseline": "…"(omitempty ptr) },
  "resolved_name": "…",                    // Claude only, reconciler-refreshed
  "wezterm":  { "mux_pid": 4790, "mux_socket": "…", "pane_id": 12, "tab_id": 7, "window_id": 3, "window_title": "…" },
  "hyprland": { "address": "0x…", "workspace": "4", "workspace_id": 4, "monitor": "" },
  "claude":   { /* AgentInfo */ },  "codex": { /* AgentInfo */ },   // exactly one
  "agent_graph": { /* AgentGraph */ }
}
```

`AgentInfo` (under `claude` or `codex`):

```jsonc
{
  "session_id": "…", "transcript": "/path.jsonl",          // omitempty
  "status": "working|idle|permission|delegating|",         // always present
  "status_since": "RFC3339",                               // derived from in-memory StatusSince; omitted until first edge
  "in_flight_subagents": 2,                                // omitempty (Claude)
  "workflows": [ { "run_id": "wf_…", "name": "…", "agents_started": 17, "agents_done": 7, "in_flight": 2 } ],
  "pending_writers": [ "af5bd…", "main" ],                 // sorted; derived from Pending keys
  "pending_prompts": [ { "writer": "main", "tool": "Bash", "input_hash": "3c81…", "call_id": "…",
                         "attention": "approval|user_input", "since": "RFC3339" } ]   // grouped by writer asc, oldest first
}
```

`AgentGraph` (child agents live only here; they are never sessions):

```jsonc
{
  "root_id": "…", "source": "codex_app_server|hook|claude_transcript|codex_rollout|restored_last_known",
  "observed_at": "RFC3339"(omitzero), "fresh_until": "RFC3339 ceiled to 5s"(omitzero), "complete": true,
  "summary": { "runtime": "…", "attention": "…", "status": "…", "live_children": 0, "waiting_nodes": 0,
               "approval_nodes": 0, "user_input_nodes": 0, "error_nodes": 0, "since": "RFC3339"(omitzero) },
  "nodes": [ { "id": "…", "parent_id": "…"(omitempty), "nickname": "…", "role": "…", "description": "…",
               "runtime": "…", "attention": "…", "lifecycle": "…",          // three axes always present
               "started_at": …, "updated_at": …, "completed_at": …,         // omitzero
               "usage": { "input_tokens": …, "cached_input_tokens": …, "cache_write_input_tokens": …,
                          "output_tokens": …, "reasoning_output_tokens": …, "total_tokens": …,
                          "model_context_window": … } } ]                    // usage omitzero, each field omitempty
}
```

**Doc/code discrepancy**: `docs/state-schema.md:83-104` documents `mem_agent_bytes` and
`mem_tree_bytes` on `Session`; `state.go` has no such fields and `rg` finds them only in the two
docs. Treat them as planned, not shipped.

## 3. Concurrency

### 3.1 Goroutine inventory

| # | Goroutine | Started at | Loop / wake source | Shutdown | Talks to |
|---|---|---|---|---|---|
| G0 | `main` | — | blocks on `<-serverErr` (`main.go:297`) | `cancel()`, `federated.Wait()`, deferred `agentRuntime.Close`, `sink.Close`, `cancel` | everything |
| G1 | signal handler | `signal.NotifyContext` (112) | SIGINT/SIGTERM | cancels root ctx | all ctx consumers |
| G2 | `history.Sink.run` | `NewSink` (`sink.go:49`) | `range s.ch` (cap 512) | `Close` closes ch, waits `done` | disk (daily jsonl) |
| G3 | `agentCoordinator.run` | `agentRuntime.Start` (191; `agent_observation.go:144`) | ticker(reconcile-interval), `requests.Updates()` (queue cap 64), claude/codex `Updates()` | `Close` cancels ctx, `wg.Wait` | Store (Snapshot/Apply), providers, sink |
| G4 | `scanner.Run` | 246-250 | ticker(scan-interval, 1 s); calls `onAgentAppeared` **inline** | ctx | resolver (3 s I/O), Store.Apply, federation announce (opens TTY), coordinator.Request, sink, `procSrc.Watch` |
| G5..n | pidfd death watchers | `procSrc.Watch` (236; `osproc.go:62-64`) | kernel pidfd readiness, fires once | ctx, or process death | Store.Apply → `endSession` |
| G6 | `store.LogPublishStats` | 257 | ticker(1 min) | ctx | log; `runtime.ReadMemStats` (stop-the-world) |
| G7 | `runWMLoop` | 258 | `manager.Subscribe` channel + 200 ms debounce timer; retry `time.After(2s)` | ctx, channel close | resolver.Enumerate (subprocess), Store.Snapshot/Apply, sink |
| G7' | WM backend reader | inside `manager.Subscribe` | Hyprland socket2 | ctx / EOF closes channel | G7 |
| G8 | `runReconciler` | 259 | ticker(5 s), first tick immediate | ctx | resolver.Enumerate, /proc, transcripts, Store.Apply, sink |
| G9 | `federation.View.RunReady` | `StartViews` (`federation.go:126`) | store subscription + remote frames | ctx; `wg` | Store.Subscribe, rpc view |
| G10 | `RunLiveRoutesReady` | `federation.go:131` | remote route frames | ctx; `wg` | panebind registry |
| G11 | `remotestate.Manager.Run` | `StartRemotes` (156), only after RPC ready | SSH subprocess streams | ctx; `wg` | dials the local socket |
| G12 | `server.ServeReady` | 284 | `Accept` loop (`rpc.go:342`) | listener closed by G13 on ctx | per-conn goroutines |
| G13 | listener closer | `rpc.go:337-340` | `<-ctx.Done()` | closes listener | G12 |
| G14/conn | `s.handle` | `rpc.go:350` | `json.Decoder` on conn | EOF, decode error, ctx | Store, handlers |
| G15/conn | conn closer | `rpc.go:366-372` | ctx or `stopClose` | — | closes conn |
| G16/sub | `connectionClosed` | `rpc.go:553` | one-byte `conn.Read` | conn close | signals subscription loop |
| G17 | `Store.runPersistence` | on demand, `state.go:1278` | drains `persistPending` batches | exits when queue empty | disk (`state.json`) |

Provider observers (Claude transcript watcher, Codex app-server child) run further goroutines
owned by the other survey. `sweepDeadSessions` (`main.go:369`) is **not** on the production
path: `reconcileOnce` inlines the same `sessionDead`/`endSession` pair (952, 996); tests use it.

### 3.2 Locks and lock order

| Lock | Guards | Order rule |
|---|---|---|
| `Store.mu` (RW) | sessions map, subscribers set, caps, `publishedKey/Gen` (`state.go:730`) | Held across the Apply closure and one change-key encode. **No I/O inside.** `SWITCHBOARD_DEBUG_LOCK=<dur>` logs holds over the threshold (817-848). |
| `Store.statsMu` | publish counters | Leaf; taken under `mu` in `countDecision` (`publishstats.go:105`). |
| `Store.broadcastMu` | `broadcastGen`, fanout, persistence enqueue (748) | Taken after `mu` is released in `Apply`; takes `mu.RLock` inside (1200, 1232). `CurrentBroadcast` takes it then calls `Snapshot()` (1316-1324). |
| `Store.frameMu` (RW) | `lastBroadcast` (752) | Leaf under `broadcastMu`. |
| `Store.persistMu` | `persistPending`, `persistRunning` (768) | Leaf under `broadcastMu`. |
| `resolveTurn.mu` | the enumerate→apply window of both resolve producers (`main.go:857`) | **Always before** `Store.Apply`, never inside. Not touched by RPC or hooks. |
| `Scanner.mu` | seen-set (`discovery.go:355`) | Leaf. |
| `federationRuntime.wg` | three federation goroutines | `Wait` after `cancel`. |

Derived ordering: `resolveTurn → Store.mu → statsMu`; `broadcastMu → Store.mu(R)`;
`broadcastMu → frameMu`; `broadcastMu → persistMu`. `Store.mu` is never held while acquiring
`broadcastMu`.

### 3.3 Timers, tickers, timeouts

- scan ticker 1 s (`discovery.go:339`), reconcile ticker 5 s (`main.go:892`), coordinator ticker
  5 s (`agent_observation.go:204`), publish-stats ticker 60 s (`publishstats.go:184`).
- WM layout debounce 200 ms, **armed by the first event of a burst, not re-armed** (`main.go:638,
  727-730`); WM subscribe retry 2 s (648).
- `Resolve`/`Reconcile`/`Enumerate` each cap their I/O with a 3 s context (`mapping.go:68, 104, 147`).
- Codex naming timeout 45 s and hook settle timers live in the coordinator (other survey).

### 3.4 Channels

`serverErr` (cap 1) and `ready` (`main.go:282-283`); per-subscriber `chan Broadcast` cap 1
(`state.go:1177`); persist waiter `chan error` cap 1 (1268); `Sink.ch` cap 512 (`sink.go:47`);
WM event channel from `Subscribe`; coordinator `InvalidationQueue` cap 64 (drops when full,
`agent_observation.go:124, 189`); `viewReady`/`routesReady` one-shots (`federation.go:122-123`).

### 3.5 Blocking I/O sites

Unix listen/accept/read/write (`rpc.go:324-471`); `state.json` temp+rename (`state.go:1339-1362`);
`Load` (1378); `/proc` reads via `osproc.Source` and `proc.State`/`proc.VmHWMKB`; transcript tail
reads (`transcript.*` from `main.go:480, 497, 1302, 1312, 1384, 1472, 1476` and `rpc.go:950, 1281`);
`wezterm cli list` and `hyprctl` subprocesses inside `resolver.Enumerate`/`Locate`; TTY open in
`AnnounceSession` (`federation.go:189`); SSH in remotestate; `runtime.ReadMemStats` once a minute.

### 3.6 Data flow

```
                 /proc scan (1s)        pidfd death        Hyprland socket2         hooks / bars / ctl
                 G4 scanner.Run         G5 watchers        G7' → G7 runWMLoop       G12 accept → G14 handle
                      |                     |               (200ms layout coalesce)       |
   onAgentAppeared ---+                     |                     |                       |
   resolver.Resolve (3s I/O) ---------------+---------------------+--- resolveTurn -------+ (hooks: Snapshot +
        |                                   |                     |     (mutex)           |  /proc walk, then
        v                                   v                     v                       |  coordinator.HandleHook)
  +---------------------------------- state.Store.Apply(fn) -----------------------------------+
  |  mu.Lock  fn(sessions)  snapshotLocked()  adoptPublishedLocked(changeKey)  mu.Unlock      |
  +------------------------------------------|-------------------------------------------------+
                        changed?             |               unchanged → return (suppressed++)
                                             v
                        broadcast(snap, gen)  [broadcastMu: reject gen <= broadcastGen]
                          | 0 subscribers: clear lastBroadcast, queue persist
                          | else: NewBroadcast (encode once) → lastBroadcast → each subscriber chan (cap 1, drop-old)
                          v
                  queuePersistence → G17 runPersistence → temp file → rename state.json
                          |                                                   |
      G14 subscribe loop: wake → writeSnapshotFrame(CurrentBroadcast)         v
      G9 federation view: wake → aggregate frame                        bars polling the file
                          |
                 clients (waybar/polybar/TUI/ctl/remote daemons)

  G8 runReconciler (5s): [resolveTurn] Enumerate → Snapshot → per-session prepare (dead? resolve,
     name, suspended, legacy self-heals, transcript tails) → Apply(copy prepared; endSession dead;
     applyFocus; prune)   — history.Sink.Record (non-blocking, cap 512) from inside and outside Apply
  G3 coordinator: Snapshot → provider observe (I/O) → Apply(SetAgentGraph) fenced by per-root generation
```

## 4. External boundaries

### 4.1 Files

| Path | Mode | Where |
|---|---|---|
| `state.json` at `$XDG_CACHE_HOME/switchboard/state.json` else `$HOME/.cache/switchboard/state.json` (`-state`) | write: `MkdirAll 0755`, `CreateTemp(".state-*.json")`, indent 2, `Rename`; read at boot | `main.go:1567`, `state.go:1339-1417` |
| History dir `$XDG_STATE_HOME/switchboard/history/<day>.jsonl` (`-history-dir`); config `$XDG_CONFIG_HOME/switchboard/history.json` | append, opt-in | `main.go:119-129`, `sink.go` |
| Claude transcripts `<proj>/<session>.jsonl` and `<session>/subagents/agent-<id>.jsonl`; Codex rollouts | read tails (`TailBytes` = 128 KiB) | `main.go:480-520, 1301-1312, 1472-1476` |
| Claude session-name file (PID-keyed) | read, memoised by mtime | `fanout.go:96`, `main.go:959` |
| `/proc/<pid>/{stat,status,cwd,exe,cmdline}` and `/proc/self/status` (VmHWM) | read | `osproc`, `proc.State` (`main.go:965`), `publishstats.go:167` |
| projectname config | read once at boot | `main.go:123` |

### 4.2 Sockets and processes

- RPC Unix socket at `$XDG_RUNTIME_DIR/switchboard.sock` else `/tmp/switchboard-<uid>.sock`
  (`-socket`); unlinked before listen and on exit, `chmod 0600` (`rpc.go:323-329`).
- Hyprland socket2 event stream and `hyprctl`/`wezterm cli` subprocesses via the `wm`/`terminal`
  seams; SSH subprocesses for `-remote` destinations.
- pidfd per discovered agent (`osproc` Linux backend).
- Signals: SIGINT, SIGTERM → context cancel (`main.go:112`).

### 4.3 Environment and flags

Env: `XDG_CACHE_HOME`, `HOME`, `XDG_RUNTIME_DIR` (`main.go:1568-1577`), `SWITCHBOARD_DEBUG_LOCK`
(`state.go:818`), `HYPRLAND_INSTANCE_SIGNATURE`/`XDG_RUNTIME_DIR/hypr`, `SWAYSOCK`, `I3SOCK`,
`DISPLAY` (`detect.go:77-91`).

Flags (`main.go:85-96`): `-state`, `-socket`, `-scan-interval` (1s), `-reconcile-interval` (5s),
`-wm auto|hyprland|sway|i3|x11|none`, `-terminal auto|wezterm|tmux|none`, `-history-dir`,
`-codex-observer auto|off`, `-codex-autoname-model`, `-version`, `-remote` (repeatable).
Link-time `-X …/buildinfo.Version=`.

### 4.4 RPC wire protocol (`rpc.go`)

One JSON object per request on the connection; responses are JSON objects, one per line.
A connection may send many requests except that `subscribe`/`subscribe-all` is terminal
(`rpc.go:397-402, 547-559`). Unknown `cmd` → `{"error":"unknown cmd: …"}`.

| `cmd` | Request fields | Response | Handler | Clients |
|---|---|---|---|---|
| `list` | — | `{"snapshot":…}` host-local | 384 | tests, ctl |
| `list-all` | — | `{"snapshot":…}` aggregate (falls back to local) | 387, 473 | `switchboard-ctl` (`main.go:886`) |
| `subscribe` | — | initial frame, then one `{"snapshot":…}` per change | 397, 511-545 | remote daemons (`remotestate/frame.go:273`), tests |
| `subscribe-all` | — | same, aggregate view | 400, 480-509 | `claude-tui`, bars (`bottombar.go`, `polybar/main.go`) |
| `focus` | `selector` ("", "active", "pid:N", "idx:N", bare N) | `{"ok":true}` / error | 390, 561-629 | ctl (legacy path) |
| `focus-session` | `hostname, pid, started_at` | ok/error | 403; federation navigator | `switchboard-ctl focus` (`main.go:219`) |
| `pane-bind` | `binding, gui_pid, window_id, pane_id` | ok/error | 415 | WezTerm Lua via ctl (`main.go:338`) |
| `pane-state` | `gui_pid, window_id, pane_id, window_focused` | ok/error | 427 | WezTerm Lua via ctl (350) |
| `announce-bindings` | — | ok/error | 439 | `ctl remote-stream` (`remote_stream.go:27`) |
| `hook` | `event, pid, session_id, observed_at, prompt, last_assistant_message, transcript, hook_source, turn_id, tool_use_id, permission_mode, agent, tool_name, tool_input_hash, agent_id, agent_type, hook_client_hints[]` | always `{"ok":true}` | 451, 693 | `switchboard-ctl hook` from Claude Code / Codex hooks (`main.go:666`) |
| `agent-diagnostics` | — | `{"ok":true,"diagnostics":[{provider,category,count,last_at}]}` | 454 | ctl (`main.go:173`) |
| `activity` | `activity: idle|active` | ok/error | 460, 1338 | hypridle via ctl (383) |

Errors are strings; two typed sentinels exist for `focus` (`ErrNavigateUnsupported`,
`ErrHeadlessSession`, 41-46). Subscription frames splice the shared `Broadcast.JSON` as a
`json.RawMessage` so the bytes equal a fresh `Response` encoding (183-188).

### 4.5 Log lines that are contracts

`build: version=…` first line (`main.go:110`); `status: pid=<n> session=<id> …` decision lines
parsed by `switchboard-ctl diagnose` (`statustune.Decision.Log`, emitted from `main.go:1212,
1229, 1498` and `rpc.go:884`); `publish-stats: …` (`publishstats.go:87`); `hook-identity:`,
`hydrate:`, `liveness sweep:`, `agent-observer:`, `remote-state:` lines are journal telemetry.

## 5. Dependencies

`cmd/switchboard` imports: agentgraph, buildinfo, detect, discovery, fanout, federation,
history, label, mapping, osproc, panebind, proc, projectname, provider/{claude,codex},
remotestate, rpc, state, statustune, terminal, transcript, wm.

`internal/state` imports only `agentgraph` and `proc` (VmHWM). `internal/rpc` imports discovery,
fanout, history, osproc, proc, state, statustune, terminal, transcript, wm. `internal/detect`
imports osproc, state, terminal, wm. `buildinfo` and `durfmt` import nothing internal.

Importers of `state` (non-test): rpc, detect, mapping, label, fanout, federation, remotestate,
barview, barlayout, waybarchip, and every `cmd/*`. Importers of `rpc`: cmd/switchboard,
switchboard-ctl, switchboard-polybar, claude-tui, remotestate (`frame.go`).

## 6. Invariants and hazards

Numbered so the Rust code can cite them.

1. **One writer of `session_end`.** Only `endSession` removes a session from the map
   (`main.go:320-332`). Four triggers (pidfd, sweep, stale-drop, window-closed) all call it; it
   is idempotent by map membership (L5) and a bare `delete` is forbidden (L7,
   `docs/session-lifecycle-hazards.md:391-399`). Test: `TestEndSessionEmitsExactlyOncePerDeath`.
2. **Liveness only from positive death evidence.** `sessionDead` returns true only on
   `osproc.ErrGone` or a pid reclassified as a non-agent; any other read error means "alive"
   (`main.go:344-353`, L4). A false end splits a lane; a late end costs one tick.
3. **Restart self-heal.** The pidfd watch dies with the daemon; the per-tick sweep is the
   durable backstop (L1/L3) and `dropStaleSessions` closes lanes that died during downtime (L2).
4. **No I/O under `Store.mu`.** Enumeration, `/proc`, transcript tails all run on detached
   copies before `Apply`; only decided values are copied back, and only when `StartedAt` still
   matches (`main.go:826, 993, 1026`). Tests: `TestShouldNotHoldTheStoreLockAcrossTerminalEnumeration`,
   `…OnTheWMLayoutPath`, `TestShouldEnumerateTheTerminalOncePerTickWhateverTheSessionCount`.
5. **Two resolve producers share one turn.** `resolveTurn` wraps enumerate-and-apply so an
   older enumeration cannot land after a newer one (`main.go:836-869`). Test:
   `TestShouldNotEnumerateConcurrentlyFromBothResolvePaths`.
6. **Layout coalescing is a rate limit, not a trailing debounce.** Arm on the first event, never
   re-arm; flush a pending re-resolve before any focus/close event and on stream close
   (`main.go:621-736`). Tests in `wm_debounce_test.go`, notably
   `TestShouldStillReresolveWhenLayoutEventsNeverStopArriving` and
   `TestShouldLandAPendingLayoutReresolveBeforeDispatchingAFocusEvent`.
7. **Publish gate = JSON of the wire contract.** `SnapshotChangeKey` encodes every tagged field
   and excludes `UpdatedAt`, `AgentGraph.ObservedAt`, `AgentNode.UpdatedAt`; `json:"-"` fields
   fall out for free (`state.go:974-1008`). Consequences: every wire projection must be
   deterministic (`pending_writers` sorted, `pending_prompts` grouped, `workflows` by RunID,
   nodes in DFS order); a title spinner must stay off the wire. Tests:
   `TestApply_publishesNothingWhenNothingChanged`, `…shouldNotRepublishWhenOnlyTheTitleSpinnerRotates`,
   `TestPendingWritersProjectionIsOrderStable`.
8. **Freshness ceiling makes suppression sound.** `fresh_until` is published rounded up to 5 s
   so a consumer that missed a suppressed republish cannot go falsely stale
   (`agent_graph.go:14-64`). Test: `TestPublishedCeilingCoversEverySuppressedInstant`.
9. **Adopt under the write lock, fan out by generation.** The change key is adopted inside
   `Apply`'s lock (mutation order); `broadcast` rejects `gen <= broadcastGen`; persistence is a
   latest-wins batch; a failed persist retracts only its own generation so the next Apply
   republishes (`state.go:855-914, 1191-1302`). Tests: `TestBroadcastRejectsLateOlderGeneration`,
   `TestBroadcastSerializesPersistenceInGenerationOrder`, `TestApply_republishesWhenTheLastPersistFailed`.
10. **Subscriber channels are wakeups.** Cap 1, drop-old-push-new; readers re-read
    `CurrentBroadcast` and never forward the queued value (`state.go:1233-1249`, `rpc.go:523-545`).
    Zero subscribers skips the encode and clears the cache (1203-1212). Tests:
    `TestBroadcastSaturationKeepsFinalReplacement`, `TestLocalSubscribeTreatsQueuedBroadcastsAsNotifications`.
11. **Snapshots are deep copies.** `snapshotLocked` clones Wezterm/Hyprland/AgentInfo/AgentGraph/
    DisplayName; `enrichForWire` stamps `status_since`, `pending_writers`, and prunes
    `pending_prompts` to the key set (`state.go:1032-1106`). Test: `TestSnapshotDetachesMutableSessionFields`.
12. **Chip order.** Resolved workspace first by id, unresolved last, then `StartedAt`, then PID;
    stable sort so the aggregate can inject a workspace key (`state.go:1123-1168`).
13. **Persistence is atomic and byte-pinned.** temp+rename in the same dir; the golden test pins
    the exact bytes of both the file and the broadcast frame (`golden_test.go`, `TestBroadcastEncodingIsTheGoldenDocument`).
14. **Hydrate rules.** Mismatched `schema_version` → ignore the mirror; v0 mirror containing a
    codex row → ignore (`state.go:1394-1405`). `pending_writers` → `Pending` map; `pending_prompts`
    writers are unioned in; `StatusSince` is never restored (`1432-1449`, `agent_graph.go:300-344`).
    `dropStaleSessions` then re-stamps `StatusSince` and every `Pending.Since` to startup,
    falsifies writers against their own transcript files, seeds "main" for a pre-T12 red, and
    re-folds a disagreeing status to `permission` (`main.go:397-610`). Tests:
    `TestHydratePendingHandlesEveryMirrorCombination`, `TestPromptStatePersistsAcrossADaemonRestart`.
15. **Never manufacture a resolution.** Hydrate may only remove ownership (a proven `tool_result`),
    never add it (`main.go:447-464`).
16. **StatusSince anchoring (H1/H7/H8).** Into `working`: anchor to the transcript entry, floored
    at the previous `StatusSince`; into `idle`/`permission`: wall-clock now (`rpc.go:926-950`,
    `docs/timing-hazards.md`). Only stamped on a real transition (`rpc.go:890`).
17. **Hookless recoveries.** `selfHealStuckStatus` (`main.go:1412-1491`): delegating from the
    subagent count before the mtime pre-gate (H6); idle-title demotion needs Claude, not
    suspended, `TitleAt > StatusSince`, idle glyph, and `IdleTitleGrace` 15 s (H9); interrupt
    marker → idle, activity → working; never a no-activity TTL (H3). `selfHealStaleAttention`
    resolves each blocked writer against its **own** file, drops per writer, and exits only when
    the map empties (`1180-1324`); the 30 min stale-writer cap applies only when the tail shows
    no unanswered `tool_use` (`1326-1388`). All of this runs only for sessions with
    `AgentGraph == nil` (`main.go:969-988`), i.e. until the Claude adapter has produced a graph.
18. **Hook attribution.** `findTrackedAncestor` walks ≤20 ppids, returns the first tracked pid,
    and returns 0 (drop) on reaching an untracked agent process — the nested `claude -p` race
    (`rpc.go:1368-1422`). `normalizeAgentID` strips at most one `agent-` prefix, once, at entry
    (1134). Empty `agent_id` means main thread. A teammate hook never drives the parent chip
    except to raise `permission` (804-806).
19. **Permission hold gate.** `clearsPermission` (1243-1304): writer must own the prompt; tool
    name + hash match clears at hook speed; hash mismatch is not a hold on its own; an
    unidentified writer with teammates in flight never clears at hook speed; hydrated entries
    (Tool == "") never fast-path. Codex is exempt. Tests across `writer_match_test.go`,
    `unidentified_writer_floor_test.go`, `permission_hold_test.go`.
20. **Session rotation.** A new `session_id` on a live pid clears every pending prompt and is
    exempt from the hold (`rpc.go:756-759, 847`); `SessionID`/`Transcript` are last-hook-wins.
21. **History from inside Apply is fine.** `Sink.Record` is non-blocking (drop on full) and
    events are recorded even when the publish is suppressed
    (`publish_suppression_invariant_test.go`: `TestShouldRecordFocusFromInsideApplyWhenApplySuppressesThePublish`).
22. **Focus events key on session id.** Empty id means no agent focused; a change is emitted
    only when the id changes (`main.go:1104-1127`).
23. **Startup ordering.** Federation views subscribe before the socket is ready; remotes start
    only after `ready` closes because they dial the local socket (`main.go:277-296`,
    `federation.go:119-162`).
24. **Capabilities re-published every tick** because the terminal locator is self-redetecting
    (`main.go:914-918`, `detect.go:102-108`).
25. **Codex user-input latch does not survive restart.** A persisted hook-sourced `user_input`
    root is reduced to unknown on load (`agent_graph.go:306-321`).
26. **Go JSON escapes `<`, `>`, `&` in strings.** The golden bytes and `RawMessage` splicing
    assume `encoding/json` defaults (`rpc.go:179-182`). A Rust encoder must match or the byte
    pins and remote-frame equality break on titles containing those characters.

## 7. Essential vs accidental

**Essential** (the problem demands it):

- A single session map with one mutation path, deep-copied snapshots, and a publish gate keyed
  on the wire contract. The gate is what keeps a 5 s reconcile from waking ten bar processes.
- The four-trigger, one-writer session-end rule and "death only from kernel evidence".
- Keeping subprocess/transcript I/O off the store lock, and serialising the two producers of the
  window mapping so writes carry the freshest enumeration.
- Atomic `state.json` plus byte-pinned golden fixtures; schema v3 field order and omit rules.
- Ceilinged `fresh_until` (sound suppression), sorted projections, advisory clocks excluded from
  the key.
- Hydrate policy: restore ownership, re-stamp clocks, falsify but never manufacture.
- Hook attribution by ppid walk that stops at an untracked agent; single-place agent-id
  normalisation.
- Layout coalescing with a maximum wait, and immediate focus/close handling.
- The JSONL socket protocol and its command set; subscription as a terminal request.

**Accidental / Go-shaped / historical** (simplify in Rust):

- **The legacy hook path is dead in production.** `main.go:264` installs `SetAgentHookHandler`, so
  `rpc.handleHook` runs `dispatchAgentHook` and returns (`rpc.go:694-696`); lines 697-1000 and
  `clearsPermission`/`pendingEntryFor`/`statusFromHookEvent` survive only for tests and
  embedders. The provider coordinator owns that logic now. Port the rules (invariants 16, 18-20)
  where the coordinator lives, not into the RPC layer.
- **The legacy fanout observer is nil in production.** `runReconciler` is called with `obs=nil`
  (`main.go:259`) and `Server.SetFanout` is never called, so `observeFanout` and the
  `SubagentStart/Stop` re-scan at `rpc.go:992` are no-ops. `reconcileState` shrinks to usage
  offsets and label cursors.
- **`selfHealStaleAttention`/`selfHealStuckStatus` are a fallback** for sessions without an agent
  graph. Decide whether the Rust adapter can always produce a graph (then delete ~400 lines of
  `main.go`) or keep them as the pre-graph window only.
- `sweepDeadSessions` is unused outside tests; `handleWMEvent`'s layout branch is unreachable
  (`main.go:785-796`); `Store.persistSnapshot` and `afterBroadcastUnlock` are test seams.
- Hand-rolled subscriber coalescing (cap-1 channel, drop-old) is exactly `tokio::sync::watch`.
- Hand-rolled persistence batching (`persistMu`, `persistRunning`, waiter slices) is a
  latest-wins queue behind one writer task.
- `broadcastGen` exists because Go lets two `Apply`s reach `broadcast` out of order after the
  unlock. Doing fanout and persist-enqueue while still holding the store lock (or under one
  publisher mutex taken inside Apply) removes the generation dance; only the encode needs to be
  cheap enough to sit there, and it already is.
- `PendingTool` (a derived scalar kept for the decision log), `PendingSummary`, `PendingCallCount`
  are log-shaped; derive at log time.
- The `ClaudeInfo` alias, `Enrichment()`'s "default" branch, and the `Claude`/`Codex` twin
  pointers are wire-compat scaffolding. In Rust, one `AgentInfo` plus `Agent` kind, serialised
  under the right key by a custom `Serialize`, is enough.
- The Go-1.23 timer-semantics comment and `pending` flag in `drainWMEvents` are Go artefacts.
- `SWITCHBOARD_DEBUG_LOCK` and `ReadMemStats` telemetry are nice-to-have, not contract.
- `durfmt` belongs to renderers, not the daemon; `buildinfo` reduces to `env!("CARGO_PKG_VERSION")`
  plus a build-script git hash.
- Federation, pane binding and the aggregate view are large and orthogonal; keep them behind a
  trait so the core compiles without them.

## 8. Rust mapping sketch

### 8.1 Runtime shape

Use **tokio** (multi-thread runtime) for everything that waits on a file descriptor or a timer:
the Unix socket server, per-connection tasks, the Hyprland socket2 reader, pidfd readiness
(`AsyncFd` over a `pidfd_open` fd), tickers, signal handling, and the layout debounce.
Keep the **store synchronous**: `parking_lot::RwLock<Sessions>` with a sync `apply` that runs
the closure, computes the change key, and hands the frame to a `watch::Sender`. The reconcile
tick and hook attribution do blocking work (spawn `wezterm cli`, read `/proc`, read transcript
tails), so run each tick in `spawn_blocking` (or a dedicated `std::thread` with a channel) and
only call `store.apply` at the end. Persistence is one `spawn_blocking` writer fed by a
`watch::Receiver<Arc<Snapshot>>` (latest-wins for free). History sink: `mpsc::channel(512)`
with `try_send` (drop on full) and one blocking writer task.

### 8.2 Module layout

```
switchboard/
  src/main.rs              flags (clap), tracing, signal → CancellationToken, wiring, shutdown order
  src/store/mod.rs         Store, apply(), snapshot(), subscribe() -> watch::Receiver<Arc<Broadcast>>
  src/store/change_key.rs  SnapshotChangeKey: serialize a redacted clone (clocks zeroed)
  src/store/persist.rs     atomic temp+rename writer task; load() with schema gate
  src/store/stats.rs       publish/suppress counters, once-a-minute line
  src/model/session.rs     Session, WeztermInfo, HyprlandInfo, AgentInfo, PendingPrompt(Record), WorkflowStatus
  src/model/graph.rs       AgentGraph, Summary, Node, Usage, ceil_fresh_until, Fresh
  src/model/display_name.rs
  src/model/order.rs       chip order comparator
  src/lifecycle.rs         end_session, session_dead, drop_stale_sessions, hydrate_pending
  src/reconcile/mod.rs     tick(): enumerate → prepare (detached) → apply; resolve_turn Mutex
  src/reconcile/selfheal.rs legacy Claude self-heals (behind a feature flag or deleted, see §7)
  src/wm_loop.rs           subscribe/retry loop + layout rate limiter
  src/rpc/mod.rs           listener, per-conn task, Request/Response, command dispatch
  src/rpc/attribution.rs   find_tracked_ancestor, normalize_agent_id
  src/detect.rs            backend selection + Capabilities
  src/buildinfo.rs
  (federation/, panebind/, remotestate/ behind traits; durfmt in the renderer crate)
```

### 8.3 Key types

```rust
pub struct Store { inner: RwLock<Inner>, publish: watch::Sender<Arc<Broadcast>>, persist_tx: watch::Sender<Arc<Snapshot>>, stats: Mutex<PublishCounters> }
struct Inner { sessions: BTreeMap<Pid, Session>, caps: Option<Capabilities>, published_key: Option<Vec<u8>> }
pub struct Broadcast { pub snapshot: Arc<Snapshot>, pub json: Option<Bytes> }   // compact encoding, shared

#[derive(Serialize, Deserialize, Clone)] pub struct Session { … }              // field order = wire order
pub enum AgentKind { Claude, Codex }                                             // serialize as "claude"/"codex"
pub enum Status { Working, Idle, Permission, Delegating, Unknown }               // Unknown ↔ ""
pub struct AgentInfo { pub session_id: String, pub transcript: String, pub status: Status,
    #[serde(skip)] pub status_since: Option<SystemTime>, /* wire: status_since via custom Serialize */
    pub in_flight_subagents: u32, pub workflows: Vec<WorkflowStatus>,
    #[serde(skip)] pub pending: BTreeMap<WriterId, PendingPrompt>,               // BTreeMap = sorted keys, no PendingWriterKeys()
    pub pending_prompts: Vec<PendingPromptRecord> }
pub struct WriterId(String);  // "" = main; Display as "main"; wire (de)serialize handles the swap

pub enum SessionEnd { PidfdDeath, LivenessSweep, StaleDrop, WindowClosed }       // for the history event/log only
pub enum WmEvent { FocusChanged(Address), WindowClosed(Address), LayoutChanged }

pub trait OsProc { fn read(&self, pid) -> Result<Info, ProcErr>; fn enumerate(); fn watch(&self, pid) -> Result<impl Future<Output=()>>; }
pub trait WmManager { fn name(); fn clients(); fn active_window(); fn focus(); fn subscribe() -> Result<mpsc::Receiver<WmEvent>>; }
pub trait TerminalLocator { fn name(); fn locate(tty); fn activate(pane); fn snapshot() -> Option<HashMap<Tty, PaneRef>>; }
pub enum Cmd { List, ListAll, Subscribe, SubscribeAll, Focus{selector}, FocusSession{..}, PaneBind{..}, PaneState{..}, AnnounceBindings, Hook(HookRequest), AgentDiagnostics, Activity(Activity) }
```

Serde notes: `omitempty` → `#[serde(skip_serializing_if = "…")]`; `omitzero` on times →
`Option<DateTime<Utc>>`; RFC 3339 with `Z` via `chrono` + `serde` feature (Go emits `Z` for UTC
and nanosecond precision only when non-zero; the golden fixtures use whole seconds); the
`fresh_until` ceiling via a custom `Serialize` on `AgentGraph`; `schema_version` omitted when 0 on
read but always written as 3. Match Go's `<>&` escaping (invariant 26) with a small custom
formatter if byte-for-byte frame equality with Go peers is required during migration.

### 8.4 Sync vs async

| Concern | Choice |
|---|---|
| Store apply/snapshot/change key | sync, `parking_lot` |
| Fanout to subscribers | `watch::Sender::send_replace(Arc<Broadcast>)` inside apply; subscribers `changed().await` then `borrow_and_clone` (this is invariant 10 by construction) |
| Persistence | one blocking task, latest-wins `watch` |
| RPC server | tokio `UnixListener`; per-conn task with `BufReader` lines + `serde_json`; subscription task selects on `watch.changed()` and a one-byte read |
| Scanner, reconciler, coordinator ticks | `tokio::time::interval` driving `spawn_blocking` bodies; hold a `tokio::sync::Mutex` (the resolve turn) across enumerate+apply |
| pidfd | `AsyncFd<OwnedFd>` readable → `store.apply(end_session)`; watchers are `JoinHandle`s cancelled by the token |
| WM events | backend task → `mpsc`; consumer loop with `tokio::time::sleep` armed once per burst |
| History sink | `mpsc::Sender::try_send` + blocking writer |
| Shutdown | `CancellationToken`; order: stop accepting, cancel workers, `JoinSet::join_all`, close sink, final persist |

Crates (conservative): `tokio` (rt-multi-thread, net, time, signal, sync, io-util), `serde`,
`serde_json`, `chrono` (serde), `parking_lot`, `tokio-util` (CancellationToken), `nix` or
`rustix` (pidfd, getuid, TTY), `clap`, `tracing` + `tracing-subscriber`, `thiserror`,
`bytes`. Add `tempfile` for the atomic write if the hand-rolled temp+rename is undesirable.
