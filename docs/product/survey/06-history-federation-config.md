# 06 — History, Federation, and the Configuration Surface

Survey of the Go implementation at `~/Projects/switchboard` (paths below are
relative to that root; `.worktrees` ignored). Read-only; nothing was modified.

Two subsystems that share nothing but a process:

- **History** (`internal/history`, 3,300 lines incl. tests): an opt-in,
  append-only JSONL activity log written by the daemon, plus the read-side
  timeline derivation (`switchboard-ctl timeline`), the fanout seed index the
  daemon replays at startup, and a calibration tool.
- **Federation** (`internal/remotestate` 944 lines, `internal/federation` 883
  lines, plus `cmd/switchboard/federation.go`, `cmd/switchboard-ctl/remote_stream.go`):
  one `ssh` child per configured peer streams that peer's full snapshot as
  JSONL; a read-only aggregate `View` merges local + remote rows; a `Navigator`
  focuses the exact local WezTerm pane that displays a remote session.

Section 5 also inventories every config file, env var, and CLI flag the whole
project reads, as requested.

---

## 1. Responsibilities

- **Record** a durable stream of session lifecycle / status-transition /
  usage / fanout / focus / activity events, one JSON line each, one file per
  local day, without ever blocking the daemon's state lock (`internal/history/sink.go:14-19`).
- **Gate and bound** that recording: off by default, `minimal` privacy tier
  scrubs content-bearing fields, retention by age and total bytes (`internal/history/config.go:10-35`, `sink.go:144-241`).
- **Replay** the log into per-session swimlanes, label/subagent/focus spans,
  token cost, and attention/delegation aggregates (`internal/history/timeline.go`).
- **Flag** lanes whose length is inference rather than observation (a lost
  `session_end`) and hold aggregates to the trusted part (`internal/history/suspect.go`).
- **Carry** names and idle/active state across day-file boundaries at read
  time (`internal/history/carryover.go`).
- **Seed** the fanout Observer at daemon start with "what has the log already
  recorded for this session" in one streaming pass plus a persisted cursor,
  so restart cost is O(appended tail) (`internal/history/seed.go`, `seedcursor.go`).
- **Project** provider-neutral agent-graph observations into canonical
  `agent_state` edges with reconnect dedupe (`internal/history/agent_state.go`).
- **Pull** remote hosts' full snapshots over `ssh <dest> switchboard-ctl remote-stream`,
  keep the latest per canonical hostname, survive one bounded reconnect
  before dropping rows (`internal/remotestate/manager.go`).
- **Merge** local + remote rows into one read-only aggregate frame served on
  `list-all` / `subscribe-all`, change-gated and encoded once per publish
  (`internal/federation/view.go`).
- **Navigate** to a remote session by resolving the exact local WezTerm pane
  bound via an OSC user-variable and raising its WM window (`internal/federation/navigator.go`, `internal/panebind`).

---

## 2. Core types

### 2.1 History — the event (domain)

`internal/history/history.go:76-144`. `ts` and `type` are the only
guaranteed-present fields; everything else is `omitempty`. `pid` is always
written in practice; `session_id` whenever known.

| JSON field | Go field | Type | Tier | Set by |
|---|---|---|---|---|
| `ts` | `Ts` | RFC3339 time | — | every writer |
| `type` | `Type` | string enum (below) | — | every writer |
| `session_id` | `SessionID` | agent session UUID | min | all but `session_start`, `activity` |
| `pid` | `PID` | int | min | all but `activity` |
| `agent` | `Agent` | `claude` \| `codex` | min | most |
| `project` | `Project` | abbreviation | min | filled in the sink goroutine from `cwd` (`sink.go:134-138`) |
| `from`, `to` | `From`, `To` | status strings; for `activity`, `To` ∈ {`idle`,`active`} | min | `transition`, `activity` |
| `rule` | `Rule` | statustune rule id | min | `transition` |
| `subagents` | `Subagents` | int in flight at the edge | min | `transition` |
| `dur_prev_ms` | `DurPrevMs` | int64 | min | `transition`, `agent_state` |
| `agent_id` | `AgentID` | `agent-<id>` stable subagent id; the correlation key | min | `subagent_*` |
| `tool_use_id` | `ToolUseID` | legacy pairing key | min | `subagent_*` |
| `agent_type` | `AgentType` | e.g. `Explore` | min | `subagent_*` |
| `background` | `Background` | bool | min | `subagent_spawn` |
| `thread_id`, `parent_thread_id` | | node ids | min | `agent_state` |
| `nickname`, `role` | | display metadata | **full** | `agent_state` |
| `from_runtime`/`to_runtime`, `from_attention`/`to_attention`, `from_lifecycle`/`to_lifecycle` | | `agentgraph.*State` | min | `agent_state` |
| `source` | `Source` | `agentgraph.SourceKind` | min | `agent_state` |
| `workflow_run_id` | `WorkflowRunID` | `wf_<hex>` | min | `workflow_*`, workflow subagents |
| `tok_in`, `tok_out`, `tok_cache_read`, `tok_cache_create` | `Tok*` | int64 | min | `usage_sample` |
| `model` | `Model` | model id | min | `usage_sample` |
| `cwd` | `CWD` | raw path | **full** | most daemon writers |
| `pending` | `Pending` | tool name a permission prompt was for | **full** | `transition` |
| `reason` | `Reason` | human detail behind a rule | **full** | `transition` |
| `description` | `Description` | subagent task text | **full** | `subagent_spawn` |
| `label` | `Label` | session name / workflow name | **full** | `session_label`, `workflow_start` |

Event kinds (`history.go:33-61`) and the exact daemon call sites that write them:

| `type` | Written at | Key payload |
|---|---|---|
| `session_start` | discovery, `cmd/switchboard/main.go:228` | pid, agent, cwd; no session id yet |
| `session_end` | single writer `endSession`, `main.go:320-332`; triggers: pidfd death-watch `:236`, liveness sweep `:997`, startup stale-drop `:397+`, WM window-closed | closes the lane, releases the pid |
| `transition` | hook path `internal/rpc/rpc.go:911`, reconciler `main.go:1080`, agent-graph status change `cmd/switchboard/agent_observation.go:576` | from/to/rule/subagents/dur_prev_ms/pending/reason |
| `suspend` / `resume` | reconciler on Ctrl-Z edge, `main.go:1018` | bounds a greyed span |
| `subagent_spawn` / `subagent_stop` | fanout Observer, `internal/fanout/observer.go:512-534`, via `cmd/switchboard/fanout.go:121` and hook-speed `rpc.go:993` | agent_id, tool_use_id, agent_type, description, background; spawn is dated at meta mtime, not now (`observer.go:513-518`) |
| `workflow_start` / `workflow_stop` | `observer.go:456-462` | workflow_run_id, label (name) |
| `agent_state` | `AgentStateProjector.Project`, `internal/history/agent_state.go:175-197`, recorded at `agent_observation.go:571` | all six axes, thread ids, source, dur_prev_ms |
| `usage_sample` | reconciler `fanout.go:152`, one per model touched since last offset; cursor primed to file size on first sight without emitting (`fanout.go:133-142`) | tok_*, model |
| `session_label` | reconciler `fanout.go:102`, deduped per session id | label |
| `focus` | `applyFocus`, `main.go:1123`; session_id empty when focus left all agent windows | session_id, pid, agent |
| `activity` | `switchboard-ctl activity idle\|active` → `rpc.go:1344`; session-less, global | to |
| `memory_sample` | **retired 2026-08-26**; readers skip the lines (`history.go:52`, `timeline.go:401`) | — |

### 2.2 History — writer, config, retention (plumbing)

- `Config` (`config.go:22-35`): `Enabled`, `Detail`, `RetainDays`, `MaxBytes`
  from the file; `Dir` (from `-history-dir`) and `ResolveProject` (a closure
  over `projectname`) injected by the daemon. Decoded through a pointer struct
  so an absent key differs from an explicit `0` (`config.go:95-100`).
- `Sink` (`sink.go:23-29`): `enabled`, `cfg`, `dir`, buffered `chan Event`
  (512, `sink.go:34`), `done`. Disabled sink is a valid zero-cost value.
- `dayFile` / `listDayFiles` / `pruneDir` / `Purge` (`sink.go:169-273`).
- `seedCursorFile` (`seedcursor.go:51-62`): `{version:1, files:{day→bytes}, sessions:{id→{spawned,stopped,wf_started,wf_stopped}}}`.

### 2.3 History — derived read model (domain)

- `Interval{Status,Start,End,Subagents}` (`timeline.go:56-61`).
- `LabelSpan`, `SubagentSpan{AgentType,Background,AgentID,ToolUseID,Description,Start,End,Suspect,SuspectReason,closedByBound}`, `FocusSpan` (`timeline.go:69-111`).
- `Swimlane` (`timeline.go:118-191`): identity (`SessionID`,`PID`,`Agent`,`Project`,`ProjectFull`,`Name`), `Start/End`, `Intervals`, `Labels`, `Names`, `Subagents`, `Focus`, per-lane tokens + `CostUSD`, `Suspect/SuspectReason/SuspectSince`, unexported `closedByBound`, `lastEvidence`.
- `Summary` (`timeline.go:663-685`): `ByStatus`, three attention figures, four delegation figures, `SuspectLanes/SuspectDuration`.
- `Totals`, `PlanWindow`, `ActivitySpan` (`timeline.go:764-810, 1002-1006`).
- `span` + `mergeSpans/intersectSpans/subtractSpans/clipSpans` — the half-open interval algebra (`timeline.go:833-1176`).
- `SuspectPolicy{End,Bound,LaneCap,SubagentCap}`, `DefaultSuspectTrailingCap = 4h`, `DefaultSuspectSubagentCap = 2h` (`suspect.go:76-114`).
- `SeedSets`, `SeedIndex`, `SeedStats`, `SeedResult` (`seed.go:29-65`, `seedcursor.go:42-47`).
- `AgentStateProjector` with per-root `nodes` + `seen` dedupe keyed `(root,node,axis,target,transitionNanos)` (`agent_state.go:24-54`).
- `Price` table keyed by family `opus|sonnet|haiku|fable` (`pricing.go:34-39`).
- `Calibration`, `Population`, `Band`, `Verdict` (`calibrate.go`) — advisory tool only.

### 2.4 Federation / remotestate (plumbing with a thin domain core)

- `remotestate.Frame{Host, Snapshot}` (`frame.go:45-48`): the wire unit, one
  JSON object per line, ≤ 4 MiB (`DefaultMaxFrameBytes`, `frame.go:26`).
- `CanonicalHostname` (`frame.go:54-72`): lowercase, trailing-dot stripped,
  DNS-label validated; the remote's own `os.Hostname()`, never the SSH alias.
- `ManagerConfig` / `Manager` (`manager.go:64-107`): `destinations`,
  `localHostname`, injectable `CommandFactory`, `FrameReader`, `RetryWaiter`;
  state maps `live`, `removing` (tombstones), `hostOwners` (host→dest),
  `destHosts` (dest→host), `subscribers`.
- `Diagnostic{Destination,Host,Category}` (`manager.go:56-60`) — finite, content-free.
- `federation.View` (`view.go:37-53`): `local *state.Store`, `remote RemoteSource`,
  `hostname`, `remoteFocus sessionKey{host,pid,startedAt,source}`, `routeReady`,
  `routeWorkspace`, `publishedKey`, cached `frame *state.Broadcast`.
- `federation.Navigator` (`navigator.go:25-33`): `View`, `panebind.Registry`,
  `RouteResolver`, `wm.Manager`, `FocusLocal`, `ActivatePane`.
- `federation.WorkspaceIndex` (`workspace.go:34-49`): last WM enumeration keyed
  by `(pid, "[sbw:gui:win]" marker, workspace)`.
- `panebind.ExactSessionKey{Hostname,PID,StartedAt}` and the OSC payload
  `{"v":1,"host","pid","started_at"}` ≤ 512 bytes (`internal/panebind/payload.go:12-29`).
- Wire fields on `state.Session` that only the aggregate sets: `Hostname`,
  `Remote`, `Navigable`, `LocalWorkspace` (`internal/state/state.go:27-63`).

---

## 3. Concurrency

### 3.1 History

| Actor | Started by | Communicates | Shutdown |
|---|---|---|---|
| **Sink writer goroutine** `Sink.run` (`sink.go:86-121`) | `NewSink` when enabled (`sink.go:49`) | receives `Event` over `ch` (cap 512); owns the open `*os.File`; calls `prune` at start and on every day rotation; calls `cfg.ResolveProject` then `scrub` then `json.Marshal` + `Write` | `Close` closes `ch`, waits `done` (`sink.go:78-84`); `defer sink.Close()` in `main.go:126` |
| `Sink.Record` | any daemon goroutine, **often under the store lock** (`main.go:239`, `rpc.go:911`) | non-blocking `select` send; drops on full buffer (`sink.go:71-74`) | — |
| `AgentStateProjector` | agent coordinator | `sync.Mutex` over `roots` (`agent_state.go:25`); `Project` and `Forget` | none |
| Fanout seed (`ensureSeedIndexLocked`, `internal/fanout/seedtelemetry.go:36-57`) | first `Observer.Reconcile` needing a seed, under `o.mu` | synchronous file streaming (`SeedLoad`), then `WriteSeedCursor` temp+rename | — |
| Reader CLI (`switchboard-ctl timeline/history`) | one-shot process | reads files directly; no daemon connection | exits |

Blocking I/O: `os.OpenFile(O_APPEND)` + `Write` in the writer goroutine only;
`os.ReadDir`/`os.Remove` in prune (same goroutine); seed scan `bufio.Reader`
reads under the Observer mutex at startup (`seed.go:132-169`); atomic cursor
write `os.WriteFile(tmp)` + `os.Rename` (`seedcursor.go:160-165`).

### 3.2 Federation / remotestate

| Actor | Started by | Communicates | Shutdown |
|---|---|---|---|
| **One worker goroutine per `-remote` destination** `runWorker` (`manager.go:240-265`) | `Manager.Run`, from `federationRuntime.StartRemotes` after the RPC socket is ready (`cmd/switchboard/federation.go:154-162`, `main.go:296`) | spawns `ssh` child (`newSSHCommand`, `manager.go:188-202`), reads its stdout via `ReadFrames`, calls `accept` → `publishLocked` fan-out on change | ctx cancel; `waitForRetry` timer between attempts (2 s default, `manager.go:19`) |
| **Attempt watcher goroutine** `watchAttempt` (`manager.go:277-315`) | per attempt | closes the child's stdout on ctx cancel or on a 10 s recovery timeout (`defaultRecoveryTimeout`) so `ReadFrames` unblocks; result over a cap-1 channel | joins when `readDone` closes |
| `ssh` **subprocess** | per attempt; `cmd.WaitDelay = 5s` | stdout only; `-n -T BatchMode ConnectTimeout=10 ServerAliveInterval=15 ServerAliveCountMax=2 StrictHostKeyChecking=yes ClearAllForwardings=yes`, `SSH_ASKPASS_REQUIRE=never` | `process.Wait()` always joined before the next attempt (`manager.go:402`) |
| `Manager.subscribers` | `Subscribe()` (`manager.go:575-589`) | `chan map[string]state.Snapshot` cap 4; on full, evict one and re-send newest (`manager.go:591-613`); deep copy via JSON round trip (`cloneSnapshotMap`, `manager.go:626-646`) | `cancel()` closes |
| `Manager.mu` (`sync.RWMutex`) | — | guards `live/removing/hostOwners/destHosts/subscribers`; `OnHostRemoved` callback runs **outside** the lock between tombstone and delete (`manager.go:494-522`) | — |
| **View run goroutine** `View.run` (`view.go:209-242`) | `StartViews` (`federation.go:121-150`), before `ServeReady` | subscribes to `Store` (`chan state.Broadcast`) and `Manager`; on either wakeup calls `publish()` | ctx cancel |
| **Live-routes goroutine** `runLiveRoutes` (`routes.go:27-69`) | `StartViews` | subscribes to `Manager`; on every wakeup re-reads `Snapshot()` and calls `registry.ReplaceLive(host, keys)`; calls `view.Refresh` | ctx cancel; on source close drops every known host |
| `View.publish` (`view.go:330-357`) | run goroutine, RPC handlers (`BindPane`, `PaneState`), `ObserveWindows` from the reconcile tick, `OnHostRemoved` from an SSH worker | `publishMu` serializes snapshot+key+frame swap; `frameMu` guards the cached frame; `v.mu` guards subscribers/focus/route closures; wakeups are `chan struct{}` cap 1, non-blocking send | — |
| `View.subscribers` | `rpc.subscribeAll` per client connection (`rpc.go:480-509`) | each wakeup → `CurrentFrame()` → splice pre-encoded bytes | connection close |
| `WorkspaceIndex.mu` | — | `ObserveWindows` from the reconcile tick; `Workspace` on the publish path | — |
| OSC announcer (`panebind.Announcer.Announce`, `internal/panebind/osc.go:71-105`) | discovery (`federation.go:224` in `main.go`) and `announce-bindings` RPC from a remote-stream attach (`federation.go:197-202`) | opens the session's PTY `O_WRONLY|O_NONBLOCK|O_NOCTTY|O_NOFOLLOW` (`tty_unix.go:31`), revalidates liveness, bounded non-blocking write of clear-then-set `OSC 1337 SetUserVar=SWITCHBOARD_SESSION=…` | synchronous, best-effort |
| `remote-stream` on the **peer** (`StreamLocal`, `frame.go:252-298`) | `ssh` from the client daemon runs `switchboard-ctl remote-stream` | sends `announce-bindings` then `subscribe` on the peer's local socket; writes one `Frame` per received snapshot to stdout | stdout EOF / SIGPIPE when the client kills ssh |

Lock order in `View`: `publishMu → frameMu`, `publishMu → v.mu`; no caller holds
`v.mu` while entering `publish()` (`docs/memory-footprint-phase1/03-task-8-federation-encode.md:230-234`).

### 3.3 Diagram

```
 peer host (nlessfun)                             client host (goosebook)
 ───────────────────                              ──────────────────────
 switchboard daemon                                switchboard daemon
   │ state.Store ──subscribe──► switchboard-ctl      ┌──────────────────────────────┐
   │                            remote-stream        │ remotestate.Manager          │
   │   announce-bindings ◄──────┘   │                │  worker[dest] ──► ssh child ──┼─── stdin/stdout ───┐
   │   (OSC to each PTY)            │ stdout JSONL   │      ▲   readFrames/accept   │                    │
   └────────────────────────────────┼───────────────►│      └── watchAttempt(10s)   │◄───────────────────┘
                                    │                │  live[host] / removing{}     │
   OSC 1337 SetUserVar ──(ssh pty)──┼───► local      │  publishLocked ─► chan map ──┼──┐
                                    │    WezTerm     └──────────────────────────────┘  │
                                    │    pane ──Lua──► switchboard-ctl pane-bind/pane-state
                                    │                        │ RPC                     │
                                    │                ┌───────▼──────────────┐         │
                                    │                │ federation.Navigator │         │
                                    │                │  panebind.Registry   │◄─ RunLiveRoutes ◄─┘
                                    │                └───────┬──────────────┘         │
                                    │                        │ Refresh/SetRemoteFocus │
   state.Store ─subscribe(chan Broadcast)─────────────►┌─────▼──────────────┐◄────────┘
                                                       │ federation.View    │  publish(): key-gate, encode once
                                                       │  CurrentFrame()    │──► rpc subscribe-all clients
                                                       └────────────────────┘      (waybar, bottombar, TUI)

 history (same client daemon)
   hook/reconcile/discovery goroutines ──Record (non-blocking, cap 512)──► Sink.run goroutine ──append──► $XDG_STATE_HOME/switchboard/history/YYYY-MM-DD.jsonl
   fanout.Observer (under o.mu) ──SeedLoad──► .seed-cursor-v1.json + tail replay ──► SeedIndex
   switchboard-ctl timeline ──ReadRange──► BuildSwimlanes → FlagSuspectLanes → MarkDelegationDormant → Summarize
```

---

## 4. External boundaries

### 4.1 History files

- Directory: `$XDG_STATE_HOME/switchboard/history` → `~/.local/state/switchboard/history` (`history.go:150-156`); override `-history-dir`.
- Day-file `YYYY-MM-DD.jsonl`, named for the **local** day (`history.go:163-165`); opened `O_APPEND|O_CREATE|O_WRONLY 0644` (`sink.go:128`); rotated when `dayKey(ev.Ts)` changes (`sink.go:102-112`). Legacy UTC-named files tolerated by readers padding one day each side (`reader.go:76-91`).
- Seed cursor `.seed-cursor-v1.json` (dot-prefixed so `listDayFiles` ignores it), mode 0600, temp+rename (`seedcursor.go:36, 160-165`).
- Retention (`sink.go:207-241`): at sink start and each rotation, delete files with date < `dayStart(now) - retain_days`, then trim oldest until total ≤ `max_bytes`, never today's file. `0` = unlimited for both.
- Torn final line: writers append `line+'\n'` in one `Write`; readers skip unparseable lines (`reader.go:39-41`); seed scan folds a torn tail but does not advance the cursor past it (`seed.go:158-164`).
- External **rewriters** of day-files: `scripts/scrub-memory-samples` and `scripts/repair-launch-ack-spans` (Python, temp+rename, backup dir, refuse while daemon runs). A shrunken file invalidates the seed cursor (`seedcursor.go:101-105`).
- Reads are file-direct; no daemon RPC serves history.

### 4.2 Privacy tiers

`minimal` (default) scrubs `cwd`, `pending`, `reason`, `description`, `label`,
`nickname`, `role` in the writer goroutine after project resolution
(`sink.go:144-156`). `full` keeps them. The scrub is a **blacklist**: any new
field survives `minimal` unless added there (`docs/history-schema.md:74-77`).
Nothing is ever transmitted; history is local-only (`docs/history-schema.md:59`).

### 4.3 Federation transport

- **Pull, not push.** The client daemon runs `ssh -n -T <dest> switchboard-ctl remote-stream` per `-remote` (`manager.go:188-202`); the peer needs `switchboard-ctl` on its non-interactive SSH PATH (`docs/machine-configuration.md:63-172`).
- **Encoding:** one `{"host":"<canonical>","snapshot":{…full state.Snapshot…}}\n` per publish (`frame.go:74-102`); `schema_version` must equal `state.CurrentSchemaVersion` (3, `internal/state/display_name.go:8`); sessions must have `pid>0`, non-zero `started_at`, unique pid (`frame.go:152-170`). Decoder rejects trailing data, null snapshot, missing `schema_version/sessions/updated_at` (`frame.go:107-150`).
- **What crosses:** the whole snapshot including `AgentGraph`; the view then nils `Wezterm`/`Hyprland` on remote rows and recomputes `Focused`/`Navigable`/`LocalWorkspace` locally (`view.go:165-181`). No action channel goes back over SSH; focus is entirely local (`navigator.go:79-135`).
- **Binding side channel:** the peer daemon writes `ESC ] 1337 ; SetUserVar=SWITCHBOARD_SESSION=<base64 json> BEL` (clear then set) into each live root's PTY (`internal/panebind/osc.go:10-65`); the local WezTerm Lua (`integrations/wezterm/switchboard.lua`) invokes `switchboard-ctl pane-bind <payload> <gui-pid> <window-id> <pane-id>` and `pane-state …` on focus changes; window titles carry `[sbw:<gui-pid>:<window-id>]` for the WM join (`workspace.go:83-87`).
- **RPC surface added:** `list-all`, `subscribe-all`, `focus-session`, `pane-bind`, `pane-state`, `announce-bindings` (`rpc.go:387-450`); `remote-stream` on the ctl side (`cmd/switchboard-ctl/main.go:94`).
- **Diagnostics:** finite categories only (`command|start|read|invalid_frame|schema_mismatch|duplicate_hostname|local_hostname|hostname_changed|disconnected|reconnecting`, `manager.go:23-35`), logged as `remote-state: destination=… host=… category=…` (`federation.go:67`). SSH stderr is never captured.

### 4.4 Deployed configuration (`hosts/`)

Only one host overlay exists: `hosts/goosebook/systemd/switchboard.service.d/20-machine.conf` sets
`SWITCHBOARD_BIN=%h/.local/share/switchboard/current/switchboard` and
`SWITCHBOARD_ARGS=-remote nlessfun`. Two sibling overlays point the Waybar
units' `SWITCHBOARD_CTL` at `current/switchboard-ctl`. `hosts/hosts_test.go`
pins: binaries resolve through `current/`, never `/releases/` or `~/go/bin`;
the goosebook overlay keeps a `-remote` peer; no variable is set by two drop-ins
in one directory (lexical-order hazard).

---

## 5. Complete configuration surface

### 5.1 Config files

| Path | Format | Consumer | Notes |
|---|---|---|---|
| `$XDG_CONFIG_HOME/switchboard/history.json` (→ `~/.config/switchboard/history.json`) | `{"enabled":bool,"detail":"minimal"\|"full","retain_days":int,"max_bytes":int}` | daemon `history.LoadConfig` (`config.go:87-130`) | absent/malformed → disabled; defaults 90 days / 100 MiB; unknown `detail` → minimal; ignored legacy `memory` key |
| `$XDG_CONFIG_HOME/switchboard/projects.json` | `{"projects":{"<abs git root>":{"canonical","full","aliases":[]}}}` (`projectname.go:281-289`) | daemon (once, for history `project`), `switchboard-ctl name`, waybar chip renderer with mtime cache (`waybarchip.go:292-320`) | user overrides layered ahead of built-in basename defaults (`projectname.go:54-61`) |
| `$XDG_CACHE_HOME/switchboard/state.json` (→ `~/.cache/…`) | frozen `state.Snapshot` | daemon writes (temp+rename); `switchboard-ctl diagnose --observer` reads (`diagnose.go:169-172`) | `-state` overrides; cache, regenerable |
| `$XDG_STATE_HOME/switchboard/history/*.jsonl`, `.seed-cursor-v1.json` | see §4.1 | daemon sink/seed; ctl readers | `-history-dir` / `--dir` override |
| `~/.config/waybar/claude.jsonc` | Waybar config | `switchboard-ctl bottombar` (`bottombar.go:223`) | `SWITCHBOARD_BOTTOM_CONFIG` overrides |
| `~/.config/polybar/switchboard.ini` | Polybar ini | `switchboard-polybar.service` | `SWITCHBOARD_POLYBAR_CONFIG` |
| `~/.claude/sessions/<pid>.json` | Claude Code's own name file | `internal/label` (`label.go:340`) | external input, not ours |
| `~/.config/systemd/user/switchboard*.service` + `*.service.d/*.conf` | systemd units/drop-ins | systemd | see env table |

### 5.2 Environment variables

| Variable | Read by | Default / meaning |
|---|---|---|
| `XDG_CONFIG_HOME` | history, projectname | `~/.config` |
| `XDG_STATE_HOME` | history `DefaultDir` | `~/.local/state` |
| `XDG_CACHE_HOME` | daemon `defaultStatePath` (`main.go:1568`), diagnose | `~/.cache` |
| `XDG_RUNTIME_DIR` | socket path (daemon `main.go:1575`, ctl `main.go:960`, polybar, tui, waybarchip), wezterm gui-sock discovery (`wezterm.go:224`), hyprland IPC (`hyprland.go:245-275`), bottombar pid/lock files (`bottombar.go:963`) | fallback `/tmp/switchboard-<uid>.sock`, `/tmp/run-<uid>` |
| `HOME` | state path fallback | — |
| `HYPRLAND_INSTANCE_SIGNATURE`, `SWAYSOCK`, `I3SOCK`, `DISPLAY`, `XAUTHORITY` | WM detection (`detect.go:82-86`, `wm/i3.go:34-62`, `wm/x11.go:32`); unit `PassEnvironment` + self-discovery in `ExecStart` | — |
| `TMUX`, `TMUX_TMPDIR` | terminal backend (`terminal/tmux.go:36-39`) | — |
| `SWITCHBOARD_BIN`, `SWITCHBOARD_ARGS` | `systemd/switchboard.service:14-15,23` | `%h/go/bin/switchboard`, empty; overlays set `-remote …` here |
| `SWITCHBOARD_CTL` | `switchboard-waybar.service:19`, `switchboard-waybar-publisher.service:13` | `%h/go/bin/switchboard-ctl` |
| `SWITCHBOARD_POLYBAR_BIN`, `SWITCHBOARD_POLYBAR_CONFIG` | `switchboard-polybar.service:15-16` | — |
| `SWITCHBOARD_WAYBAR_MARKER` | bottombar (`bottombar.go:217`) | `/tmp/hypr-float-center/waybar-hidden` |
| `SWITCHBOARD_BOTTOM_CONFIG` | bottombar (`bottombar.go:223`) | `~/.config/waybar/claude.jsonc` |
| `SWITCHBOARD_WORKING_COLOR`, `_IDLE_COLOR`, `_PERMISSION_COLOR`, `_UNKNOWN_COLOR` | `cmd/switchboard-polybar` | polybar `env-` passthrough |
| `SWITCHBOARD_DEBUG_LOCK` | `state.go:818` | Go duration; lock-hold debug |
| `SWITCHBOARD_LIVE_CONFORMANCE` | `conformance.go:29` | opt-in live WM/terminal tests |
| `SWITCHBOARD_DEPLOY_ROOT`, `SWITCHBOARD_LINK_DIR` | `scripts/deploy` | `~/.local/share/switchboard`, `~/.local/bin` (empty disables links) |
| `SSH_ASKPASS_REQUIRE=never` | **set** by the manager on the ssh child (`manager.go:199`) | — |
| `GOMEMLIMIT=512MiB` | set in the unit (`switchboard.service:36`); `MemoryHigh=1G`, `MemoryMax=2G` | containment after the seed fix |
| `UPDATE_GOLDEN` | tests | — |

### 5.3 CLI flags

**`switchboard` daemon** (`cmd/switchboard/main.go:85-95`): `-state` (state.json path), `-socket`, `-scan-interval` (1s), `-reconcile-interval` (5s), `-wm auto|hyprland|sway|i3|x11|none`, `-terminal auto|wezterm|tmux|none`, `-history-dir`, `-codex-observer auto|off`, `-codex-autoname-model`, `-version`, `-remote <ssh-dest>` (repeatable; rejects empty; `federation.go:19-30`).

**`switchboard-ctl`** globals (`cmd/switchboard-ctl/main.go:27-29`): `--socket`, `--json`, `--version`. Subcommand flags: `timeline --dir --day --since --until --width(48) --json --no-color --plan-window --suspect-cap(4h; 0 disables both caps)` (`timeline.go:36-45`); `history path|tail(--dir --day -n 20 --json)|stat(--dir)|purge(--dir --before --all)|calibrate(--dir)` (`history.go:69-345`); `seed-bench --dir --sessions --storm --top --cursor` (`seedbench.go:41-45`); `diagnose --around --window(2m) --since --until --session --pid --unit --system --file --symptom --json --limit(200) --observer --state` (`diagnose.go:34-47`); `name resolve|abbrev|full|set|set-full --cwd --name`; positional `activity idle|active`, `pane-bind <payload> <gui> <win> <pane>`, `pane-state <gui> <win> <pane> <focused>`, `remote-stream`, `focus host:<h>:pid:<n>`.

**`switchboard-waybar`** (`waybarchip.go:111-114`): `-socket`, `-slot`, `-width-px`, `-version`. **`switchboard-polybar`**: `-socket`. **`claude-tui`**: `-socket`, `-once`.

---

## 6. Dependencies

**History imports:** `internal/agentgraph` only (`history.go:27`) — the package is
deliberately a leaf: no `state`, no `projectname` (status strings are
re-declared as literals, `timeline.go:14-19`; `ProjectFull` is filled by ctl,
`timeline.go:123-128`, `cmd/switchboard-ctl/timeline.go:125-128`).

**History imported by:** `cmd/switchboard` (main, fanout, agent_observation),
`internal/rpc` (hook transitions, activity), `internal/fanout` (event
constructors + seeding), `cmd/switchboard-ctl` (timeline, history, seedbench).

**remotestate imports:** `internal/rpc` (Request/Response for `StreamLocal`),
`internal/state`. **federation imports:** `state`, `panebind`, `wezterm`, `wm`.
**Imported by:** `cmd/switchboard/federation.go` (wiring), `internal/rpc`
(`SnapshotView` interface, `rpc.go:220-224`), `cmd/switchboard-ctl/remote_stream.go`.

The `View` is constructed and installed **unconditionally**, `-remote` or not
(`federation.go:81, 114`; `docs/memory-footprint-phase1/03-task-8-federation-encode.md:17-24`). Every waybar slot and the bottom bar consume `subscribe-all`, so the federation View is on the hot path of every local render even with zero peers.

---

## 7. Invariants and hazards

### 7.1 History write side

- **Never block the daemon**: `Record` is called under the store lock; drop on full buffer (`sink.go:71-74`). Consequence: a `session_end` can be lost (`docs/session-lifecycle-hazards.md:275-277`).
- **One `session_end` per death** (L5): `endSession` is the single writer, idempotent by map membership (`main.go:305-332`). Never bare-delete a session (L7).
- **Rediscovery re-emits `session_start`** on a live pid; readers treat a `session_start` on an open lane as continuation (`timeline.go:415-424`). Daemon restart also re-emits every live session's label (`carryover.go:84-86`).
- **Timestamps are `time.Now()`** except `subagent_spawn` (meta mtime, clamped ≤ now, `observer.go:513-518`) and `agent_state` (provider `completed_at`/`updated_at`/`started_at`, `agent_state.go:228-239`). So a day-file is **not** strictly time-ordered; every reader sorts (`timeline.go:317`, `agent_state.go:135`).
- **Day partition is local time**; DST and a machine timezone change move file boundaries. Readers filter by event `ts`, so correctness holds; only file selection pads (`reader.go:76-91`).
- **`agent_state` dedupe is in-memory per root lifetime**; a restart re-emits the initial canonical state (`agent_state.go:139-149`). Tests: `TestAgentStateProjectorPinsDurationAndReconnectDedupe`, `…ForgetScopesDedupeToRootLifetime`.
- **Usage cursor primes without emitting** on first sight, so a pre-existing transcript's backlog is never dumped as a spike (`fanout.go:133-142`); a daemon restart therefore loses tokens spent during downtime.

### 7.2 History read side

- **Group by session id, fall back to pid** (`timeline.go:283-314`). A new id on a live pid closes the old lane; one id across two pids is one lane; id after `session_end` is a new lane. Tests: `TestBuildSwimlanesSplitsSequentialSessionsOnOnePid`, `…MergesOneSessionAcrossPids`, `…ResumeAfterDeathIsTwoLanes`, `…ContinuesAcrossDaemonRestart`, `…SplitsWhenIdChangesAcrossRestart`.
- **Focus, activity, and legacy memory_sample never route to lanes** (`timeline.go:401-403`); `TestFlagSuspectLanesIgnoresMemorySamples`.
- **Pipeline order is a contract**: `ReadRange → CarriedActivityState seed → BuildSwimlanes → BackfillCarriedNames → FlagSuspectLanes → MarkDelegationDormant → Summarize` (`cmd/switchboard-ctl/timeline.go:57-101`; `docs/history-schema.md:484-493`). Out of order, flags and aggregates contradict.
- **Suspect predicate** = unclosed-by-bound ∧ silent ≥ cap measured from last evidence, not interval start (`suspect.go:194-257`); `TestFlagSuspectLanesUsesTheLastEvidenceNotTheIntervalStart`. Caps 4h / 2h are calibrated constants pinned by `TestSuspectCapDefaults`; `WithLaneCap` scales both. Known false positive: a lane crossing midnight in a `--day` query (`session-lifecycle-hazards.md:367-374`).
- **Summarize holds totals to `suspect_since`** and reports `suspect_duration`; this silently changed the meaning of every `summary` figure when it landed (`docs/history-schema.md:467-482`).
- **Active holds expire after 2h** (`activeHoldCap`, `timeline.go:42`); idle never expires; state before the first edge is unknown, not presumed active (`timeline.go:952-992, 1019-1065`). Carried activity/name lookback is 7 day-files (`carryover.go:38`).
- **Retention is size-before-age in effect** per the schema doc, but code order is age then size (`sink.go:214-240`); either way today's file is never trimmed.
- **Seed cursor is a cache, never authoritative**: any file shorter than its offset → silent full rescan; a missing file keeps its contribution (`seedcursor.go:96-105`). `TestSeedLoadCursorRoundTripMatchesFullScan`, `…RebuildsWhenADayFileShrank`, `…NeverConsumesATornTail`.
- **Seed markers match the quoted type value, not `"type":"x"`**, because repaired files carry `"type": "x"` with a space (`seed.go:78-90`); `TestSeedScanToleratesPythonSpacedRepairLines`.
- **`ReadRange` materializes the whole window**; a full-history `timeline` still peaks ~1.5 GB post-scrub (`docs/seed-replay-memory-plan.md:244-247`). The 2026-08-26 OOM came from the old per-session double `ReadRange` at seed time.
- **Prices are hard-coded** and go stale (`pricing.go:34-39`); unknown model → $0, never an error.

### 7.3 Federation

- **Full replacement, never delta.** Every frame is a complete snapshot; channel values are wakeups, consumers re-read `Snapshot()`/`CurrentFrame()` (`routes.go:59-63`, `view.go:312-315`, `rpc.go:501-504`). Tests: `TestRunLiveRoutesTreatsQueuedValuesAsNotifications`, `TestViewCurrentFrameNeverPrecedesAnEarlierRead`.
- **Clock skew is sidestepped**: the aggregate stamps `UpdatedAt: time.Now()` locally rather than max(remote) (`view.go:129-131`); `started_at` from a remote is compared only for equality as a PID-reuse fence, never ordered against local time; `SnapshotChangeKey` excludes `UpdatedAt`, `ObservedAt`, node `UpdatedAt` (`state.go:974-1008`).
- **Hostname is claimed sticky per destination**; a second destination reporting the same host is rejected (`ErrDuplicateHost`), a destination whose host changes is rejected (`ErrHostnameChanged`), a peer claiming the local hostname is rejected (`ErrLocalHostname`) (`manager.go:431-470`). Tests: `TestManagerHostnameClaimIsStickyAcrossDisconnect`, `…RejectsHostnameChangeAndRemovesPriorRows`, `…RejectsLocalHostnameBeforePublishing`.
- **Transport loss ≠ host gone**: rows survive one confirmation attempt (10 s to first frame); protocol errors remove immediately; recovery with an observably identical frame does not publish (`manager.go:321-408, 417-429`). Tests: `…ConfirmsRecoveryWithoutFlickerAndNeverOverlapsChildren`, `…FailedConfirmationAttemptRemovesOnce`, `…BoundsAConfirmationAttemptThatNeverSendsAFrame`, `…RetainsRowsWhileDisconnectedChildReaps`.
- **Tombstone before callback**: `removing[host]` hides the host from every `Snapshot()` while `OnHostRemoved` drops routes outside the lock (`manager.go:494-505`); `TestManagerTombstonePreventsAnotherSourceRepublishingRemovingHost`.
- **Workers never overlap children**; `Wait()` is joined before the next attempt (`manager.go:402`); Manager is single-use (`ErrManagerAlreadyRun`).
- **A binding authorizes nothing**; liveness from the latest full frame is the gate, and navigation revalidates pane, marker, and WM window at action time (`navigator.go:79-135`, `routes.go:71-95`). `RouteReady` is a hint only.
- **Remote `Focused` is derived locally** from WezTerm pane-state + WM active window, source-scoped so a late unfocus cannot clear a newer window (`view.go:254-290`, `navigator.go:194-255`); `TestLateUnfocusCannotClearNewerWindow`.
- **Remote rows are ordered by the local workspace showing them**, resolved once per publish from the last WM enumeration; a failed enumeration (nil) is ignored, an empty one replaces (`workspace.go:57-78`, `view.go:156-192`).
- **History is never federated**: remote sessions do not reach the Sink; `remote-stream` subscribes to the peer's local `Store`, not its `View`, preventing loops (`rpc.go:217-219`).
- **Startup ordering**: `StartViews` (view + live-routes subscribed and first frame published) → `ServeReady` → `StartRemotes` (`main.go:277-296`), because `remote-stream`'s attach dials the peer socket immediately and an OSC callback may arrive before the first remote frame.
- **Unverified in the field** as of the plan doc: end-to-end OSC delivery through an ordinary SSH PTY while Claude owns the alt screen, clear-then-set producing a fresh event, SSHMUX (`docs/ssh-federation-plan.md:524-541`).

---

## 8. Essential vs accidental

### History

**Essential (carry into v1):**
- The event schema of §2.1 as an on-disk contract. A dashboard (`switchboard-dashboard`) already consumes `timeline --json`; the JSONL format is greppable, crash-tolerant, and dependency-free. Keep day-files, local-day naming, `omitempty` additive fields, the two privacy tiers, the retention rules.
- The non-blocking sink with a bounded queue. The design decision that a slow disk drops events rather than stalling the daemon is correct for a desktop daemon; the cost (lost `session_end`) is already backstopped on the read side.
- `BuildSwimlanes` + `FlagSuspectLanes` + `Summarize` and the span algebra. These encode months of corpus-derived semantics (pid/session non-1:1, rediscovery, suspect caps, active-hold expiry). Port them faithfully with the same fixtures; do not redesign.
- The seed index + cursor for the fanout observer. Whether the Rust rewrite keeps a transcript-diffing fanout observer is a discovery-subsystem question, but if it does, "what did I already record" must be answerable in O(tail).

**Accidental / droppable:**
- `agent_state` running **alongside** legacy `subagent_spawn/stop` and `transition` (a migration overlap, `history.go:57-60`). v1 should pick the canonical `agent_state` stream and derive legacy views from it, or keep legacy only.
- `memory_sample` tolerance code, the `carriedNameLookbackDays` scan of raw files, `Calibrate` (advisory, unwired), `seed-bench`, `HumanBytes`, the Python repair scripts. Keep skipping unknown `type`s generically; drop the special-casing.
- Hard-coded pricing. Make it a small config file or accept `$0` for everything in v1.
- `DurPrevMs` on events is redundant with adjacent timestamps (the schema doc says so, `usage-history-plan.md:171-172`); keep for compatibility, never rely on it.

**Could history be a pure consumer of the state broadcast?** Mostly yes, and
that is the right shape for the rewrite. Today history has **eight** write
sites scattered across hook handling, the reconciler, the fanout observer, the
agent coordinator, the WM loop, and the RPC activity command. Everything a
`transition`, `session_start/end`, `suspend/resume`, `session_label`, `focus`,
and `agent_state` event carries is derivable by diffing consecutive `Snapshot`s
(status, `status_since`, `in_flight_subagents`, label, focus, agent graph
nodes). Three things are **not** in the snapshot and would need a side channel
or new snapshot fields: `usage_sample` token deltas (transcript byte offsets),
`subagent_spawn/stop` with `agent_id`/`agent_type`/`description` (the observer
reads `subagents/*.meta.json`), and the `rule`/`reason`/`pending` explanations
on a transition. Recommendation: make history a subscriber of the internal
broadcast for lifecycle/status/label/focus/graph edges, and have the discovery
subsystem emit the two content-bearing streams (usage, fanout) as explicit
typed events onto the same bus. The `activity` edge is already an external
input via RPC. The one hard constraint to preserve: the broadcast is
change-gated (`SnapshotChangeKey`), so every field history needs must be in
the key or history must subscribe to the un-gated apply stream.

### Federation

**Is it worth carrying into v1?** No, not as built. Evidence:

- Exactly one deployed peer (`hosts/goosebook`, `-remote nlessfun`), and the
  plan doc's own status is "live two-host/WezTerm acceptance pending"
  (`docs/ssh-federation-plan.md:3, 524-541`). The novel seam (OSC through an
  SSH PTY under an alt-screen app) has never been proven.
- The remote-navigation half (panebind registry, OSC announcer, WezTerm Lua,
  workspace index, navigator revalidation) is ~2,000 lines of carefully
  fail-closed machinery for a feature nobody has used end to end.
- The View is on the hot path of every local renderer even with zero peers,
  and it forced a second change gate and frame cache to be bolted on
  (task 8) to stop a wake storm.

**What to keep as a design constraint** so federation can be added later
without a rewrite: (1) sessions are namespaced by `(hostname, pid)` with
`started_at` as the stale-action fence; (2) the snapshot is a complete
replacement and the wire format is one JSON object per line, so a peer stream
is just another `Snapshot` source; (3) the local store never absorbs remote
rows; (4) hostname is asserted by the peer, canonicalized, and unique. If
v1 ships a `remote-stream`-equivalent producer (trivial: subscribe + print),
the consumer side can be written when there is a second machine to test on.
Recommended v1 scope: **observe-only federation at most** (remote rows
rendered, `navigable: false`), and no OSC/pane binding.

---

## 9. Rust mapping sketch

### Crates
`serde` + `serde_json` (events, frames, config), `time` or `chrono` with
`Local` for day keys (must respect the system zone; `jiff` is a good fit),
`tokio` for the daemon (mpsc for the sink, `process::Command` for ssh,
`select!` for the attempt watcher), `thiserror`, `tempfile` (atomic cursor
write), `nix` for the O_NONBLOCK PTY open if OSC announcement is kept.

### Modules

```
history/
  event.rs        Event { ts, kind: EventKind, session: SessionRef, payload: Payload }
                  serde with #[serde(tag="type")] into one flat struct on the wire
                  (keep the flat JSON; model it as an enum internally + a From impl)
  config.rs       HistoryConfig { enabled, detail: Detail, retain_days, max_bytes }
                  Option<T> fields on the file struct to distinguish absent from 0
  sink.rs         Sink { tx: mpsc::Sender<Event> } with try_send (drop on full),
                  writer task owning File + current_day; prune on rotation
  reader.rs       read_day / read_range (streaming iterator, not Vec<Event>)
  timeline.rs     Swimlane, Interval, Summary; BuildSwimlanes port with the same
                  lane-key routing; span algebra over (SystemTime, SystemTime)
  suspect.rs      SuspectPolicy, flag_suspect_lanes; consts 4h/2h
  carryover.rs    carried names / activity (or fold into a "window seed" step)
  seed.rs         SeedIndex, seed_scan (prefilter bytes then serde into a 5-field struct),
                  cursor v1 file (same JSON shape for compatibility)
  pricing.rs      table + cost_usd(model, tokens)
  agent_state.rs  AgentStateProjector { roots: HashMap<RootKey, RootLane> } behind a Mutex
federation/       (defer; if kept:)
  frame.rs        Frame { host, snapshot }; encode/decode with the same validation
  manager.rs      one tokio task per destination; Child via tokio::process with
                  kill_on_drop; watch::Sender<HostMap> instead of a chan of maps
  view.rs         Aggregate { local: watch::Receiver<Broadcast>, remote: watch::Receiver<HostMap> }
                  → publishes Arc<Broadcast> on a watch channel; change key = serialized
                  projection with observed_at/updated_at zeroed
  navigator.rs, workspace.rs, panebind/  only with a real second host to test against
config/
  paths.rs        xdg dirs: config/state/cache/runtime with the same fallbacks
  env.rs          the §5.2 table as a typed struct read once at startup
  cli.rs          clap: the §5.3 flags; `-remote` as Vec<String>
```

### Sync vs async
- **Sink writer**: a dedicated OS thread or a tokio task with `spawn_blocking`
  for the file append; the producer side must be `try_send` from sync code
  (it is called under the state lock).
- **Timeline derivation**: pure, synchronous, no I/O beyond `read_range`; make
  it a library usable from the CLI and the dashboard.
- **Seed scan**: synchronous at startup (the ordering constraint "no emission
  before seed" makes async pointless, `docs/seed-replay-memory-plan.md:28-30`).
- **Remote manager**: async; each worker is `loop { attempt().await; sleep }`
  with `tokio::select!` replacing `watchAttempt`; a `watch::Sender<Arc<HostMap>>`
  gives the "newest replacement wins, wakeups coalesce" semantics for free and
  removes the evict-and-resend dance in `publishLocked`.
- **View**: `watch` channels on both inputs; `publish` under one `Mutex` for
  the key gate + frame swap; serve `subscribe-all` by cloning the `Arc<Bytes>`.

### Types worth changing
- Replace `Sink`'s `ResolveProject func(string) string` with a trait object or
  resolve `project` at the call site; the cwd→abbrev lookup off the hot path
  is the only reason it lives in the writer.
- `Swimlane.closed_by_bound` / `last_evidence` become a `LaneClose` enum
  (`Observed(Instant)` | `AtBound(Instant)`) so the suspect check reads off a
  type instead of two private bools.
- Model `Event` internally as `enum EventKind { Transition{..}, SessionStart{..}, … }`
  and keep the flat `omitempty` JSON as the serialization only; unknown `type`
  values deserialize to `Unknown` and are skipped, which subsumes `memory_sample`.
- `Frame` decoding: validate `schema_version` before touching the body, exactly
  as now; keep a hard byte cap.
