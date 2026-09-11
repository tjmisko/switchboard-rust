# 05 — Clients and renderers

Read-only survey of the Go tree at `~/Projects/switchboard` (ignoring `.worktrees/`).
Scope: `cmd/switchboard-ctl`, `cmd/claude-tui`, `cmd/switchboard-waybar`,
`cmd/switchboard-polybar`, `internal/{waybarchip,barlayout,barview,statustune}`,
the `waybar/`, `polybar/`, `systemd/`, `hosts/`, `scripts/` trees, the bar and
machine docs, and the live hook wiring in `~/.claude/settings.json` and
`~/.codex/hooks.json`. `focus`/`cycle`/`attention` are listed but analysed by the
navigation survey. All paths below are relative to `~/Projects/switchboard` unless
they start with `~`.

## 1. Responsibilities

- Be the **one-shot RPC client** for humans and scripts: `list`, `status`, `pick`
  (fzf/rofi feed), `focus`, `cycle`, `attention`, `agent-diagnostics`, WezTerm
  `pane-bind`/`pane-state` callbacks, `activity idle|active` (hypridle) — all in
  `cmd/switchboard-ctl/main.go:85-138`.
- Be the **hook edge** for both agents: `switchboard-ctl hook <Event>` (Claude) and
  `codex-hook <Event>` read the hook JSON from stdin, hash `tool_input`, attach
  `getppid()`, and forward a content-free `rpc.Request{Cmd:"hook"}`
  (`main.go:629-640`). This is the privacy boundary: raw tool input, prompts and
  assistant text never cross the socket except two bounded Codex naming edges.
- Be the **Waybar broker**: `switchboard-ctl bottombar publish|watch` holds one
  `subscribe-all` stream, renders ten chip JSON documents into
  `$XDG_RUNTIME_DIR/switchboard/slot-N.json`, and pokes Waybar with
  `SIGRTMIN+N` via a pidfd (`cmd/switchboard-ctl/bottombar.go`).
- Own the **bottom-bar lifecycle invariant** `runs ⇔ top visible ∧ sessions>0`
  (`bottombar.go:265`), including start/stop of a dedicated Waybar in legacy mode
  and SIGUSR2-toggling a combined Waybar in attached mode.
- Render sessions for **Polybar** (one tail=true line, `%{A1:...}` click tags) and
  for a **terminal** (`claude-tui`, hand-rolled ANSI alt-screen).
- Provide **offline forensic tools** that need no daemon: `diagnose` (journal +
  `state.json`), `history` (day-files), `timeline` (swimlanes + attention JSON
  contract for the dashboard), `seed-bench` (memory benchmark), `name` (project
  abbreviation config).
- Carry the shared **status→colour vocabulary** (`internal/barview`), the
  **width-fitting** and **agent-tree** presentation math (`internal/barlayout`),
  and the **status-decision log grammar + tuning knobs** (`internal/statustune`).
- Ship **desktop integration assets**: `waybar/claude.jsonc`, `polybar/switchboard.ini`,
  four systemd user units, a per-host drop-in overlay, and `scripts/deploy` (atomic
  `current` symlink releases with running-revision verification).

### Binaries and subcommands

| Binary / subcommand | Needs daemon? | Purpose | Where |
|---|---|---|---|
| `switchboard-ctl list [--json]` | yes | human or JSON snapshot dump | `main.go:141` |
| `switchboard-ctl status` | yes | one-line session count | `main.go:395` |
| `switchboard-ctl pick` | yes | `host:H:pid:P:started:T \t label \t ws \t cwd` rows for fzf/rofi | `main.go:405` |
| `switchboard-ctl focus [selector]` | yes | exact federated focus (nav survey) | `main.go:207` |
| `switchboard-ctl cycle next\|prev` | yes | ring navigation skipping headless (nav survey) | `main.go:434` |
| `switchboard-ctl attention` | yes | jump to most urgent tier, one-layer ceiling (nav survey) | `main.go:497` |
| `switchboard-ctl agent-diagnostics [--json]` | yes | bounded provider health counters | `main.go:172` |
| `switchboard-ctl pane-bind PAYLOAD gui win pane` | yes | WezTerm Lua binding callback | `main.go:331` |
| `switchboard-ctl pane-state gui win pane focused` | yes | WezTerm active-pane/focus callback | `main.go:341` |
| `switchboard-ctl activity idle\|active` | yes | global user-activity edge from hypridle | `main.go:382` |
| `switchboard-ctl hook <Event>` | yes (silent) | Claude Code hook forwarder | `main.go:120,629` |
| `switchboard-ctl codex-hook <Event>` | yes (silent) | Codex hook forwarder (+ TTY hints) | `main.go:125,629` |
| `switchboard-ctl remote-stream` | yes | JSONL of local snapshots for `ssh -T host ...` federation | `remote_stream.go:14` |
| `switchboard-ctl bottombar publish` | tolerant | attach to combined Waybar, render slots, toggle via SIGUSR2 | `bottombar.go:246` |
| `switchboard-ctl bottombar reconcile-attached` | tolerant | one-shot visibility reconcile (F8 handler) | `bottombar.go:249` |
| `switchboard-ctl bottombar watch` | tolerant | legacy: own a separate `waybar -c claude.jsonc` | `bottombar.go:244` |
| `switchboard-ctl bottombar reconcile` | tolerant | legacy one-shot | `bottombar.go:242` |
| `switchboard-ctl bottombar stop` | no | legacy kill | `bottombar.go:252` |
| `switchboard-ctl diagnose [flags] [words]` | no | wrong-colour forensics from journal; `--observer` from `state.json` | `diagnose.go:31` |
| `switchboard-ctl history path\|tail\|stat\|purge\|calibrate` | no | activity-log management | `history.go:27` |
| `switchboard-ctl timeline [--day\|--since/--until] [--json]` | no | swimlanes + attention metrics; JSON contract for dashboard | `timeline.go:34` |
| `switchboard-ctl seed-bench` | no | fanout seeding cost (VmHWM, CPU) as one JSON object | `seedbench.go:39` |
| `switchboard-ctl name resolve\|abbrev\|full\|set\|set-full` | no | project abbreviation config | `main.go:825` |
| `switchboard-ctl -version` | no | build stamp for deploy smoke test | `main.go:36` |
| `switchboard-waybar [--slot N] [--width-px]` | tolerant | standalone/debug chip renderer (not in live config) | `cmd/switchboard-waybar/main.go:8` → `waybarchip.Main` |
| `switchboard-polybar [--max-sessions] [--ctl] [--*-color]` | tolerant | Polybar tail=true renderer | `cmd/switchboard-polybar/main.go:39` |
| `claude-tui [-once]` | tolerant | ANSI live view / one plain frame | `cmd/claude-tui/main.go:31` |

## 2. Core types

Domain (would survive a rewrite):

- `barview.Chip{Session, Status, Classes, Selector}` and `barview.Prepare`,
  `Status`, `ColorClass`, `FocusSelector` (`internal/barview/barview.go:17-98`).
  The single bar-neutral definition of effective status (enrichment → graph
  summary → "unknown"), CSS class set (`working|idle|permission|unknown` plus
  `delegating`, `focused`, `suspended`, `headless`, `remote`, `unnavigable`), and
  the click selector (`pid:N` local, `host:H:pid:N` remote-navigable, "" otherwise).
- `barlayout.Metrics{CharPx 8.05, ChipFixedPx 24}` and `Fit(labels, availPx, m)`
  (`internal/barlayout/barlayout.go:28,61,86`): max-min-fair rune budget with
  ellipsis, min 3 runes, deterministic `grow` round-robin (`:169`). Pure.
- `barlayout.AgentRow{Node, Depth, TreePrefix}` + `AgentRows`, `LimitAgentRows`,
  `AgentName` (36-rune cap), `AgentStateText`, `AgentStateKind`, `AgentStateAt`,
  `AgentUsageText` (`internal/barlayout/agenttree.go:17-246`). Cycle-safe tree
  prefix builder over the daemon's DFS-ordered graph nodes.
- `waybarchip.Output{Text, Tooltip, Class []string, Alt}` — the Waybar custom
  module JSON document (`internal/waybarchip/waybarchip.go:41-50`).
- `waybarchip.Renderer{availPx, metrics, names nameConfig, labels label.NameCache}`
  with `RenderSlotsAt(snap, slots, now) []Output` and `NextRefresh` (`:55-105`).
- `rpc.Request` hook fields: `Event, PID, SessionID, ObservedAt, Prompt,
  LastAssistantMessage, Transcript, HookSource, TurnID, ToolUseID, PermissionMode,
  Agent, ToolName, ToolInputHash, AgentID, AgentType, HookClientHints`
  (`internal/rpc/rpc.go:67-128`); `rpc.Response{Snapshot, Diagnostics, OK, Error}` (`:166`).
- `statustune.Tuning` (11 knobs, `internal/statustune/statustune.go:40-137`),
  `Decision.Log()` (`:162-187`), rule-id constants (`knobs.go:8-181`),
  `ruleKnobs` map (`knobs.go:197-243`), `Record`/`ParseDecision` (`parse.go:20-92`).
- `durfmt.Compact/Coarse/NextCompactChange/NextCoarseChange/Earlier` and the
  `Clock`/`Timer` interfaces (`internal/durfmt/durfmt.go:16,59`, `clock.go:7-89`).
- `timeline.go` output envelope `{window, lanes, summary, totals, activity,
  plan_window, agent_timeline}` (`timeline.go:135-143`) plus `agentTimeline`
  types (`:158-209`). This is a **frozen contract** consumed by
  `switchboard-dashboard` (memory plan hard invariant; requirements FR-O7).

Plumbing (Go-specific or replaceable):

- `bottomBarConfig` + injectable `bottomBarOps{isRunning,start,stop}`
  (`bottombar.go:55-76`), `slotPublisher{dir,last,dirty,blocked,replace,signal}`
  (`:82-89`), `bottomProcess{pid,pidfd,started}` (`:708`), `attachedVisibility` (`:764`).
- `nameConfig` (path+mtime+size memo of `projectname.Load`) duplicated verbatim in
  `waybarchip.go:292-320` and `switchboard-polybar/main.go:243-264`.
- `emitter{w,last}` byte-dedupe writer duplicated in `waybarchip.go:784-806` and
  `switchboard-polybar/main.go:268-285`.
- `hookPayload` stdin struct (`main.go:642-659`), `noPIDError`, `symptom` table
  (`diagnose.go:579-611`), `observerDiagnostic` (`diagnose.go:130-153`).

## 3. Concurrency

Every goroutine, channel, timer and blocking call, by binary.

### switchboard-ctl one-shots (list/focus/pick/hook/...)

Single goroutine. `rpc.Dial` then synchronous `Send`/`Recv` (`main.go:85-138`).
`cmdHook` blocks on `io.ReadAll(os.Stdin)` (`main.go:630`), then `Send`+`Recv`
with all errors discarded (`:637-639`) so a dead daemon never blocks the agent.
Claude's `settings.json` gives each hook a 2 s timeout (`~/.claude/settings.json:136`
et seq.), so worst case is a 2 s stall per hook if the socket accepts but never replies.

### switchboard-ctl remote-stream

Single goroutine: `announceRemoteBindings` does one `announce-bindings`
round-trip (`remote_stream.go:26-39`), then `remotestate.StreamLocal` sends
`subscribe` and loops `Recv` → validate → `EncodeFrame(stdout)` forever
(`internal/remotestate/frame.go:252-...`). Stops when stdout breaks (SSH drops)
or the daemon closes. No timers.

### switchboard-ctl bottombar publish / watch  (the Waybar publisher path)

```
                          main goroutine                                    goroutines
 cmdBottombar ──► watchBottomBar (bottombar.go:324)
   │  preseed 10 empty slot files (stage, :330)
   │  [!attached] go reapChildren() ─────────────────────────────────► Wait4(-1) loop, sleeps 1s on ECHILD (:938)
   └─ for { streamSnapshots(); stage(empty); flushSlotsWhenReady(); reconcile(); sleep 2s }  (:342-354)
          │
          ▼ streamSnapshots (:359)
          rpc.Dial ─ Send subscribe-all
          snapshots := make(chan Snapshot, 1)
          go receiveBottomSnapshots(c, cmd, snapshots) ──────────────► Recv loop; on "unknown cmd: subscribe-all"
          │                                                             re-Sends "subscribe" (:517-524);
          │                                                             latest-wins: drain+replace if full (:533-541);
          │                                                             defer close(snapshots) on EOF/err (:510)
          │  safety := time.NewTicker(3s)            (:379)
          │  refreshTimer *time.Timer (coarse clock boundary from Renderer.NextRefresh)  (:457-462)
          │  readyTimer   *time.Timer (exp backoff 25ms<<n, cap 400ms, 6 tries, then 3s)  (:438-448, :495-504)
          │
          └─ select loop (:466-487):
               snapshots ─► latest=snap; render(now)
               refreshC  ─► render(now)
               readyC    ─► scheduleReady()
               safety.C  ─► haveLatest ? render(now) : reconcile(cfg)+scheduleReady()

 render(now) (:450):  publisher.stage(renderer.RenderSlotsAt(latest,10,now))   ← write slot-N.json via tmp+rename
                      reconcileWith(cfg, len(sessions))                        ← flock(bottombar.lock) held
                      scheduleReady() → flushSlotsWhenReady → flock → openConfiguredProcess (pidfd_open)
                                        → bottomBarReady (reads bottom-waybar.ready) → PidfdSendSignal(SIGRTMIN+N+1)
                      refreshTimer.Reset(NextRefresh - now)

 Waybar side (waybar/claude.jsonc:23-32): module N runs `IFS= read -r line < slot-N.json` once at
 startup and again on SIGRTMIN+N; slot 0 also writes "$PPID <starttime>" to bottom-waybar.ready.
```

Shutdown: none. The process is killed by systemd (`Restart=on-failure`,
`PartOf=graphical-session.target`); there is no signal handler, no context, and
the reaper goroutine and receive goroutine leak on exit by design. A dropped
daemon socket returns from `streamSnapshots` (channel closed), blanks the
widgets, and retries every 2 s (`:346-353`).

Blocking I/O in this path: `rpc.Recv` (goroutine), `flock` (`mustFlock`, `:948`,
blocking `LOCK_EX`), `os.ReadFile` of `/proc/<pid>/{comm,cmdline,stat}` and the
ready/pid/visible files, `exec.Command("waybar")` start (`:670`),
`exec.Command("hyprctl monitors -j")` once at `NewRenderer` (`barlayout.go:225`),
and a synchronous `list-all` round-trip inside `reconcile` (`:286`) executed
**outside** the flock on purpose (`:283-291`).

Mutexes: none in ctl. The flock file `$XDG_RUNTIME_DIR/switchboard/bottombar.lock`
is the only mutual exclusion, shared with the F8 one-shot (`bottombar reconcile[-attached]`)
and `bottombar stop`. `Renderer` and `slotPublisher` are documented as
single-owner, no-mutex (`waybarchip.go:52-54`, `bottombar.go:81`).
`label.NameCache` does carry a `sync.Mutex` (`internal/label/label.go:423-427`)
because it is also used by the daemon.

### switchboard-waybar (standalone) and claude-tui

Identical shape (`waybarchip.go:164-267`, `claude-tui/main.go:107-201`):

```
 main loop: for { runOnce/streamInto; emit degraded; sleep 2s (waybar) / 1s (tui) }
   rpc.Dial; Send subscribe-all; out.forget()
   snapshots := make(chan Snapshot,1); errs := make(chan error,1)
   ctx, cancel := context.WithCancel(...)
   go receiveSnapshots(ctx, c, snapshots, errs)   ← blocks in Recv; selects on ctx.Done to avoid leaking
   runSnapshotRenderer(...): select { snapshots → latest, render; timerC → render; errs → return }
       timer := clock.NewTimer(next - now)   (durfmt.Timer, injectable for tests)
```

Differences: the TUI has `signal.NotifyContext(SIGINT, SIGTERM)` (`main.go:36`),
a goroutine that prints `showCursor+altScreenLeave` on cancel (`:82-85`), and a
goroutine per connection that `c.Close()`s on cancel (`:113-116`). The TUI wakes
per second while any age is under a minute (`NextCompactChange`), the waybar
renderer never faster than per minute (`NextCoarseChange`). Waybar output is
stdout JSON lines; TUI output is a full `clearHome + body` repaint (`:256`).

### switchboard-polybar

Single goroutine, no timers, no clock refresh: `for { runOnce; emit ✕; sleep 2s }`
(`main.go:69-73`), `runOnce` loops `Recv` synchronously (`:93-112`) with the
same `subscribe-all` → `subscribe` fallback (`:99-104`). Polybar has no tooltip,
so there is no live age to tick.

### Daemon side, as the clients see it

`subscribeAll` writes the current frame immediately, then one frame per edge
on a `view.Subscribe()` channel, always re-reading `CurrentFrame()` so a queued
edge cannot replay stale state (`internal/rpc/rpc.go:480-509`). A subscription is
the final request on its connection; the server spawns `connectionClosed` which
blocks on a 1-byte `Read` — any further client write is a protocol violation and
ends the stream (`:551-559`). The federation `View.Subscribe` channel is 4-deep
drop-oldest (memory plan §1.2, `README.md:128`), so clients must treat frames as
full replacements and never count them.

### The hook path

```
 Claude Code / Codex process (pid P)
   │ spawns `switchboard-ctl hook <Event>` with JSON on stdin (2 s timeout, settings.json:129-206)
   ▼
 cmdHook (main.go:629)
   io.ReadAll(stdin) ─► parseHookPayload (:665)  ─► hashToolInput (:778): canonical JSON → sha256[:16]
                                                 ─► Codex only: truncatePrompt(1000 runes) on UserPromptSubmit/Stop (:684-689)
   req.PID = getppid()                (:632)   ← identity of the agent, not the hook shell
   req.ObservedAt = now (UTC)         (:633)
   Codex: req.HookClientHints = {tty via /proc/self/fd/{0,1,2} readlink, SSH_TTY, WEZTERM_PANE, TMUX_PANE} (:709-746)
   c.Send(req); c.Recv(&resp)  errors ignored (:637-639)
   ▼ exit 0 always
```

## 4. External boundaries

### ctl ↔ daemon protocol (as used by clients)

Newline-delimited JSON over `$XDG_RUNTIME_DIR/switchboard.sock` (fallback
`/tmp/switchboard-<uid>.sock`, `main.go:959-964`, duplicated in every binary).
Commands the clients send: `list-all` (fallback `list` on
`unknown cmd: list-all`, `bottombar.go:599-627`), `subscribe-all` (fallback
`subscribe`), `subscribe` (remote-stream, `frame.go`), `focus-session`
`{hostname,pid,started_at}`, `announce-bindings`, `agent-diagnostics`,
`pane-bind {binding,gui_pid,window_id,pane_id}`, `pane-state {..., window_focused}`,
`activity {activity}`, `hook {...}`. Responses always `rpc.Response`; a non-empty
`error` string is the only failure signal. Version skew is handled by string-matching
`"unknown cmd: <name>"` (`bottombar.go:629-631`).

### Hook stdin JSON shapes

Fields read (`main.go:642-659`): `session_id`, `transcript_path`, `tool_name`,
`tool_input` (raw, hashed only), `agent_id`/`agentId`, `agent_type`/`agentType`,
`prompt`, `last_assistant_message`, `source`, `turn_id`, `tool_use_id`,
`permission_mode`. Empty or malformed body still yields a sendable
`{cmd:"hook", event, agent}` (`:667-673`; tests `hookinput_test.go:417,430`).
Events wired live: Claude `SessionStart, UserPromptSubmit, Stop, PermissionRequest,
PreToolUse (matcher AskUserQuestion|ExitPlanMode), PostToolUse`
(`~/.claude/settings.json:129-206`, timeout 2 s each; `SessionEnd` goes to a
different project's script). Codex: those plus `SubagentStart, SubagentStop`
(`~/.codex/hooks.json:9-86`). Both point at
`~/.local/share/switchboard/current/switchboard-ctl`, the deploy symlink.

### Waybar JSON

One object per line: `{"text": "...", "tooltip": "<pango>", "class": ["working","focused",...], "alt": "working"}`
(`waybarchip.go:41-46`). Empty slot: `{"text":"","class":["empty"]}`. Degraded
aggregate: `{"text":"✕","tooltip":"switchboard not running","class":["tracker-down"]}`
(`:139`). No `percentage` field is used. Tooltip is pango markup, escaped with
`pangoEscape` (`:708`); lines are identity (smallcaps name + dimmed cwd), host +
coloured dot + status text, task, `up · started · ws · pid`, and the agent
roll-up (`:438-521`). `alt` is `barview.ColorClass(status)`.

### Polybar line

`%{F#hex}● label%{F-}` per chip joined by two spaces; navigable chips wrapped in
`%{A1:'<ctl>' focus pid\:N:}...%{A}` with every colon backslash-escaped
(`switchboard-polybar/main.go:167-185`); overflow `+N`; unavailable `✕`. Text is
sanitised: CR/LF → space, `%` → `％` (`:215`). Colours validated as
`#RGB|#RRGGBB|#AARRGGBB` (`:226`). Module config `polybar/switchboard.ini:41-56`
passes the palette through `env-*` vars and binds right-click → `pick | rofi`,
scroll → `cycle`.

### TUI frame

CRLF-terminated ANSI: header `switchboard · N sessions · observe|navigate · wm= term=`
(`claude-tui/main.go:298,421`), one row per session
`* ● status(11) cwd(40) pid ws age blocked:` (`:358`), then agent tree rows with
`├─/└─` prefixes bounded at 32 rows / depth 6 (`:252-253,369-404`). Golden files
`cmd/claude-tui/testdata/*.golden.txt`.

### systemd units

- `systemd/switchboard-waybar-publisher.service`: `ConditionPathExists=%h/.config/waybar/.switchboard-combined-v1`
  (`:9`), `Environment=SWITCHBOARD_CTL=%h/go/bin/switchboard-ctl`, `XDG_RUNTIME_DIR=%t`,
  `GOGC=50` (`:13-15`), `ExecStart=/bin/sh -c 'exec "$$0" bottombar publish' ${SWITCHBOARD_CTL}` (`:16`).
- `systemd/switchboard-waybar.service`: same but `bottombar watch`, requires
  `claude.jsonc` and the **absence** of the combined marker (`:9,13`). Deliberately
  no `After=switchboard.service` (startup cycle through `graphical-session.target`, `:4-7`).
- `systemd/switchboard-polybar.service`: runs `polybar` itself, not our binary (`:14-16`).
- `systemd/switchboard.service`: daemon, `WantedBy=default.target` not the graphical
  target (`:43-51`), `GOMEMLIMIT=512MiB MemoryHigh=1G MemoryMax=2G` (`:39-41`).
- Host overlay `hosts/goosebook/systemd/*/20-machine.conf` repoints `SWITCHBOARD_CTL`
  and `SWITCHBOARD_BIN` at `~/.local/share/switchboard/current/...` and adds
  `-remote nlessfun`. Tests `systemd/service_test.go:36-152` and
  `hosts/hosts_test.go:34-83` pin every one of these strings.

### Environment variables

`XDG_RUNTIME_DIR` (socket, slot dir, lock, pid/ready/visible files),
`SWITCHBOARD_WAYBAR_MARKER` (default `/tmp/hypr-float-center/waybar-hidden`,
`bottombar.go:217`), `SWITCHBOARD_BOTTOM_CONFIG` (`:223`), `XDG_CONFIG_HOME`
(projectname config, history opt-in), `XDG_STATE_HOME` (history dir),
`XDG_CACHE_HOME` (`state.json` for `diagnose --observer`, `diagnose.go:168`),
`HOME`, `SSH_TTY`/`WEZTERM_PANE`/`TMUX_PANE` (Codex hook hints), `GOGC`.
Deploy: `SWITCHBOARD_DEPLOY_ROOT`, `SWITCHBOARD_LINK_DIR` (`scripts/deploy:54,63`).

## 5. The bottombar

**What.** A broker that keeps ten real GTK Waybar modules (per-chip CSS and click
targets) fed without ten resident renderer processes. It replaced the Phase-1
topology of `10 × switchboard-waybar --slot N` (12 MB each) plus a watcher
(memory plan §1, table at `memory-footprint-plan.md:53-57`; Phase 2 tasks #16-#18).

**Why.** Two owners drive one invariant: the F8 master toggle (marker file, owned
by `hypr-float-center`) and the daemon's session count. The bottom strip must
appear only when both say yes, and Waybar's row must never see a torn JSON line.

**Lifecycle rules** (all in `bottombar.go`):

- **should-run**: `shouldRun(topVisible, count) = topVisible && count > 0` (`:265`);
  `topVisible` is the *absence* of the marker (`:592`). Pinned by
  `TestShouldRun`, `TestReconcileWithF8TruthTable` (`bottombar_test.go:43,162`).
- **no-flap**: if the daemon is unreachable, `reconcile` leaves the bar as-is
  rather than stopping it (`:286-291`, `TestReconcileDaemonUnreachableLeavesAsIs`).
  Top hidden is decided without dialing (`:274-280`). The flock is never held
  across the socket round-trip (`:283-291`).
- **idempotence**: `ensureStarted` no-ops when `isRunning` (`:634-641`);
  attached mode records `pid starttime visible` in `bottom-waybar.visible` and
  only toggles when the recorded state differs for the *same* process generation
  (`reconcileAttachedVisibility`, `:810-826`;
  `TestAttachedVisibilityReconcileIsIdempotentAndResetsOnWaybarRestart`).
  A failed toggle is not committed (`:817-820`).
- **identity fencing**: pid files carry `pid starttime` (`/proc/<pid>/stat` field 22,
  `:917-933`); every open validates `comm == waybar`, the `-c <config>` argv in
  legacy mode (`:895-907`), and starttime equality, binding a **pidfd** before
  validation so signals target the kernel object not a recyclable number
  (`:828-881`). Attached mode requires the 2-field ready record and never adopts a
  PID-only record (`:733-736`). Legacy PID-only records are upgraded in place (`:871-879`).
- **process group kill**: `startBottom` spawns `waybar` with `Setsid` (`:671`),
  stdin from `/dev/null`, stdout/err inherited so systemd captures launch errors
  (`:702-706`). On a failed start it `Kill(-pid, SIGKILL)` the whole group, then
  `Wait` (`:680-686`). Normal stop is `PidfdSendSignal(SIGTERM)` then removing pid
  and ready files (`:651-660`). Attached mode never starts or kills Waybar (`:721-724`).
- **reaper**: legacy mode runs `reapChildren` (`Wait4(-1)` forever, `:938-946`)
  because Waybar is started with `Release()` and never `Wait`ed, and a one-shot
  F8 reconcile cannot reap a child it did not spawn (`:334-340`).
- **self-heal**: 3 s safety ticker re-renders or re-reconciles (`:379,479-486`);
  on stream drop the slots are blanked, flushed, and `reconcile` runs once before
  the 2 s retry (`:346-353`); readiness polling is exponential and bounded
  (`:495-504`, `TestWaybarReadinessPollingIsExponentialAndBounded`), after which it
  warns once and probes every 3 s (`:438-446`).
- **ready handshake**: no RT signal is sent until `bottom-waybar.ready` names the
  exact pid (+ starttime in attached mode) of the process bound by pidfd
  (`:570-588`); dirty bits survive the wait so startup edges catch up
  (`TestSlotPublisherWaitsForExactWaybarReadyPIDThenCatchesUp`).
- **atomic publish**: `stage` marshals all ten, replaces only byte-changed files
  via tmp+rename (`:113-152`, `replaceFile :182`), marks dirty only after a
  successful rename, and on a partial failure sets `blocked` so the next stage
  rewrites the whole row before any signal (`:136-140`,
  `TestSlotPublisherSuppressesPartialGenerationUntilFullRetry`).
- **signal mapping**: Waybar `signal: N` ⇒ `SIGRTMIN+N`; glibc `SIGRTMIN=34`, so
  slot s sends `34+s+1` (`:98-102`). Linux-only; other OSes get a stub (`bottombar_other.go:9`).

## 6. Rate limiting and churn rules for bar output

- **Daemon-side change key** suppresses republishes when only clocks moved
  (`updated_at`, `observed_at`, spinner frames) and quantizes `fresh_until` to a
  5 s bucket (`docs/memory-footprint-phase1/02-tasks-5-6-7-change-key.md:1-40,297-`).
  Clients therefore cannot rely on frames as a heartbeat: **renderer-owned clocks**
  (task #8.5, `memory-footprint-plan.md:812-837`, README `:333-347`) keep the
  displayed ages moving.
- **Tooltip at most once a minute**: every hover field is event-driven or
  `durfmt.Coarse` (`<1m`, then whole minutes; `durfmt.go:45-64`), because a
  tooltip rewrite makes Waybar re-render the module and dismiss an open hover
  (`waybarchip.go:433-437`). Measured motivation: 14 lines in 12 s, 13 tooltip-only
  (`durfmt.go:52-55`). Tests `TestSessionTooltipDoesNotChangeAtSecondResolution`,
  `TestQuietWaybarSnapshotAdvancesAtCoarseBoundaryWithoutAnotherSocketRead`.
- **Adaptive timer, not a poll**: `NextRefresh` takes the earliest
  `NextCoarseChange` over status-since and started-at across all slots
  (`waybarchip.go:99-105,367-382`, `durfmt/clock.go:71-80`); the TUI uses
  `NextCompactChange` (per second under a minute) plus graph `FreshUntil`
  (`claude-tui/main.go:203-234`).
- **Agent roll-up counts only finished agents** so the "done" duration is
  event-driven; live agents would rewrite the tooltip every few seconds
  (`waybarchip.go:631-636`, `TestAgentFanoutExcludesRunningAgentsFromCumulativeTime`).
- **Blocked-writer names go in the tooltip, never the chip text**, because chip
  text is fitted as a set and a label that grows/shrinks would re-abbreviate every
  other chip twice per prompt (`waybarchip.go:476-481`,
  `TestRenderSlotShouldNotPutTheBlockedWriterInTheChipLabel`).
- **Byte-identical dedupe** at the write edge: `emitter.emit` drops a line equal
  to the last (`waybarchip.go:791-800`, polybar `:274-281`); `slotPublisher.stage`
  compares marshalled bytes per slot (`bottombar.go:129`). `forget()` is called on
  every *successful* subscription but **not** on a failed dial, so a down daemon
  prints the degraded chip once, not every 2 s (`waybarchip.go:173-180`,
  `TestEmitterShouldWriteTheDegradedChipOnceWhenTheDaemonGoesDown`).
- **Coalescing**: the broker's receive goroutine keeps only the newest snapshot
  when the render loop lags (`bottombar.go:533-541`); the daemon's View channel is
  4-deep drop-oldest; the TUI/standalone renderers use a 1-deep channel with
  blocking send (so they backpressure the socket reader goroutine, not the daemon).
- **Width fit computed once**: `hyprctl monitors -j` is forked once per process,
  not per slot (`waybarchip.go:62-69,145-162`); `--width-px` overrides it.
- **Name lookups memoised** by file stamp (`nameConfig`, `label.NameCache`,
  bounded at 512 entries, `label.go:449`), so the ~1 s render cadence costs a
  `stat` not a read+unmarshal.

## 7. Dependencies

In (what the clients import): `internal/rpc` (Dial/Send/Recv), `internal/state`
(Snapshot, Session, AgentGraph, Status*), `internal/label` (Chip, RawName,
BlockedWriters, NameCache — reads Claude session files and `agent-*.meta.json`
from disk), `internal/projectname` (config under `XDG_CONFIG_HOME`),
`internal/durfmt`, `internal/buildinfo`, `internal/history` (ReadDay/ReadRange,
BuildSwimlanes, Summarize, SeedScan, Calibrate...), `internal/agentgraph`
(lifecycle/runtime/attention enums), `internal/remotestate` (StreamLocal),
`internal/statustune`, `golang.org/x/sys/unix` (pidfd, flock, signals).

Out (who consumes the clients): Waybar (`claude.jsonc` on-click/scroll → `focus idx:N`,
`cycle`, and user scripts `claude-picker`, `claude-abbrev-edit`), Polybar
(`switchboard.ini`), Hyprland binds (attention/cycle/picker), hypridle (`activity`),
WezTerm Lua (`pane-bind`/`pane-state`), Claude Code and Codex hook runners, the
federation daemon on a peer (`ssh -n -T host switchboard-ctl remote-stream`,
`docs/machine-configuration.md:63-171`), `switchboard-dashboard`
(`timeline --json`), `scripts/deploy` (`-version`), and bench scripts
(`scripts/sb-bench-seed` → `seed-bench`, `sb-mem-baseline` → raw `subscribe-all`).

External processes forked: `hyprctl monitors -j`, `waybar -c ...`, `journalctl`
(`diagnose.go:437`).

## 8. Invariants and hazards

Invariants (from tests and doc comments):

- Hook payloads are content-free on the wire: raw `tool_input` never leaves
  `parseHookPayload`; empty hash means "no signal", never "mismatch"
  (`main.go:773-777`; `TestHashToolInputShouldNotLeakRawInputWhenInputIsSensitive`,
  `TestCmdHookShouldNotPutRawToolInputOnTheWireWhenPayloadCarriesSecrets`).
  Codex naming context is admitted only on `UserPromptSubmit`/`Stop`, capped at
  1000 code points (`TestCmdHookForwardsBoundedCodexNamingContextOnlyOnItsLifecycleEdges`).
- The hook's `PID` is `getppid()`, so the hook must be spawned **directly** by the
  agent process (a wrapper shell would break attribution).
- Every slot renders the **whole** snapshot's labels through `Fit` so all chips
  agree on abbreviation (`waybarchip.go:326-331`); `Fit` is pure and stable
  (`barlayout.go:6-8`, `TestFitStaysWithinAvailableWidthWithRealMetrics`).
- Remote and local chips share one `ChipFixedPx` because the CSS trades padding
  for the double border (`barlayout.go:47-55`,
  `TestRenderSlotFitIgnoresWhetherSessionsAreRemote`).
- Remote sessions never resolve CWD or PID on the local filesystem
  (`TestRemoteTooltipDoesNotResolveCWDOnLocalFilesystem`; TUI `:326-329`).
- `delegating` paints green everywhere (`barview.ColorClass`, TUI `:264-269`,
  polybar `:205`).
- `history --json` and `timeline --json` are stable contracts; the human view may
  dedupe legacy projections but raw JSON must not
  (`history.go:111-113`, `TestHistoryTailKeepsBothRawEventsButDedupesHumanProjection`,
  `TestTimelineJSONMixedHistoryAddsCanonicalSectionWithoutChangingLegacy`).
- Renderer units are opt-in, mutually exclusive by marker, never `WantedBy=default.target`,
  and never `After=switchboard.service` (`service_test.go:68-152`).
- Deploy must fail loudly when the running process does not report the shipped
  revision, and command names on PATH must be symlinks (`scripts/deploy:17-24,281-299,369-390`).
- `TestRuleKnobCoverage` parses `knobs.go` so every `Rule*` constant has a knob row.

Hazards worth carrying into the rewrite:

- **PID-based identities everywhere**: hook attribution by `getppid`, `pick`/`focus`
  selectors by pid, `focus idx:N` by snapshot position (a snapshot reorder between
  render and click focuses the wrong session; the daemon's `StartedAt` fence only
  helps when the selector carries it — `idx:` and `pid:` do not).
- **Three files, one flock**: pid/ready/visible files under `$XDG_RUNTIME_DIR`
  parsed by hand; a stale or 1-field record is silently ignored or deleted.
- **glibc `SIGRTMIN=34` hard-coded** (`bottombar.go:52`); musl differs.
- **Version skew is string-matched** (`unknown cmd: ...`); no protocol version field.
- **`subscribe-all` is EOF-sensitive**: closing the client's write half ends the
  stream (`rpc.go:551-559`; `scripts/sb-mem-baseline` header hazard).
- **`bottombar watch` never exits and leaks goroutines**; `reapChildren` calls
  `Wait4(-1)` which will also reap any other child the process ever spawns
  (e.g. `hyprctl`), racing `exec.Command.Output`. Today the only other fork happens
  before the reaper starts, which is fragile.
- **Duplicated code**: `nameConfig`, `emitter`, `defaultSocketPath`, `sessionStatus`,
  `statusSince`, `plural` exist in 2-4 copies.
- **Daemon suppression vs. renderer freshness**: the TUI is the only renderer
  still calling `AgentGraph.Fresh(now)`; a suppressed within-bucket republish must
  not read as stale (`TestShouldNotMarkTheTUIStaleWhenTheDaemonSuppressedAWithinBucketRepublish`).
- `diagnose` shells out to `journalctl --user -u switchboard.service -o short-iso`
  and parses Go `log` prefixes; a Rust daemon logging in a different format breaks
  `extractTime`/`ParseDecision` unless the `status: pid=` grammar is preserved.
- `timeline` reads `projectname.Load()` for lane display names and warns via `log`
  on stderr; JSON on stdout must stay clean for the dashboard.

## 9. Essential vs accidental

Essential (must exist in some form):

- The hook forwarder with its hash/truncate/getppid rules. It is tiny and the
  most security-sensitive code in the tree.
- One live-subscription renderer core: *retain latest snapshot → compute next
  visible boundary → render → dedupe bytes → emit*. Waybar, Polybar and TUI are
  three formatters over that one loop.
- `barview.Prepare`/`Status`/`ColorClass`/`FocusSelector` and `barlayout.Fit`
  as pure functions.
- The bottombar invariant, atomic slot files, pidfd-fenced signalling, ready
  handshake, and attached-mode idempotence fence.
- `pick`, `list --json`, `remote-stream`, `timeline --json` as script contracts.
- `diagnose`/`history`/`timeline` as offline readers of the log and journal.

Accidental (Go history, migration residue, or duplication):

- **Three binaries that could be one.** `switchboard-waybar` is already a one-line
  wrapper around `waybarchip.Main` and is not in the live config; `claude-tui` and
  `switchboard-polybar` share the exact subscribe/reconnect skeleton with it. Fold
  all three into `switchboard-ctl` subcommands (`ctl waybar [--slot]`, `ctl polybar`,
  `ctl tui`). Deploy already ships `switchboard-waybar` only to keep a name alive
  (`scripts/deploy:57`).
- **Legacy `bottombar watch`/`reconcile`/`stop`** (own a second Waybar process,
  argv matching, reaper, process-group kill) exist to make the combined-config
  migration reversible. The attached `publish`/`reconcile-attached` pair is the
  target; the legacy trio can be dropped once the goosebook cutover is permanent.
- **Aggregate mode** in `waybarchip` (`renderAggregate`, `:392`) and `--slot` mode
  of the standalone binary are debug leftovers.
- **Polybar `nameConfig`/`emitter` duplicates**, and the five copies of
  `defaultSocketPath`.
- **`seed-bench`** lives in ctl only because it must call the daemon's seeding
  code path; in Rust it is a `cargo bench` or an internal `--bench` flag of the daemon crate.
- **`name` subcommand** is a config editor for `projectname`; keep but it is
  unrelated to the socket.
- **Renderers that could become pure functions over `state.json`**: the bar docs
  already show jq/eww/i3blocks recipes over `~/.cache/switchboard/state.json`
  (`docs/bars/README.md:1-32,153-222`). Polybar and the aggregate Waybar chip
  need nothing the file lacks. Only the slot broker needs live signalling
  (to avoid Waybar `interval` polling) and the TUI needs the RPC only for lower
  latency. A Rust design could make every renderer `fn(&Snapshot, now) -> Output`
  and choose the transport (socket subscribe, inotify on `state.json`, or one-shot)
  at the edge.
- **statustune** is daemon policy that lives in a "client" package only so
  `diagnose` can print knob names. In Rust the rule ids and knob table belong to
  the core crate; `diagnose` imports them.

## 10. Rust mapping sketch

**One binary, `switchboard-ctl`, with clap subcommands**; keep the daemon a
separate binary. Rationale: every client shares `rpc`, `state`, `label`,
`durfmt`; the desktop config already references only `switchboard-ctl`; the
deploy symlink set shrinks to two names. Keep `switchboard-waybar` and
`claude-tui` as thin shell shims or clap aliases for one release if scripts need them.

```
switchboard-ctl
├── list | status | pick | focus | cycle | attention | agent-diagnostics   (sync, one shot)
├── pane-bind | pane-state | activity                                     (sync, one shot)
├── hook <Event> | codex-hook <Event>                                     (sync, stdin → socket, never fails)
├── remote-stream                                                         (sync loop, stdout JSONL)
├── bottombar {publish, reconcile-attached, [watch, reconcile, stop]}     (async or threads; Linux only)
├── waybar [--slot N] | polybar | tui [--once]                            (shared subscribe loop + formatter)
├── diagnose | history | timeline | seed-bench | name                     (sync, file/journal only)
└── --version
```

Sync vs async:

- **One-shots and hooks: sync, `std::os::unix::net::UnixStream`**, blocking JSON
  lines. The hook path must start in microseconds and never block the agent; no
  runtime spin-up. Set a read timeout (~1.5 s) so a wedged daemon cannot eat the
  2 s hook budget.
- **Subscribe renderers (waybar/polybar/tui) and `remote-stream`: sync with two
  threads** (socket reader → `std::sync::mpsc::sync_channel(1)` with latest-wins,
  main loop `select`-ing via `recv_timeout` against the next boundary). Tokio is
  not needed; the Go code's `select` over one channel plus one timer maps cleanly
  onto `recv_timeout`. Inject a `Clock` trait for tests as Go does.
- **bottombar publish**: same two-thread shape plus a 3 s safety tick. Use `nix`
  (or `rustix`) for `pidfd_open`, `pidfd_send_signal`, `flock`, `setsid`,
  `waitpid`, and read `/proc/<pid>/stat` field 22 by hand as today. Gate the
  module behind `#[cfg(target_os = "linux")]`. Replace `Wait4(-1)` with
  `waitpid(child_pid)` on the specific child, or spawn Waybar via
  `Command::spawn` and keep the `Child` to avoid reaping strangers.
- If the daemon crate ends up on tokio anyway, the renderers can share its
  `rpc::Client` async type behind a small blocking wrapper; do not make the hook
  path depend on it.

Crates: `clap` (derive), `serde`/`serde_json` (all wire and file shapes; use
`serde_json::to_vec` on a `BTreeMap`-based `Value` for the canonical
`tool_input` hash, `sha2` for the digest), `nix` or `rustix` (pidfd, flock,
signals, getppid), `time` or `chrono` (RFC3339 parsing in selectors and journal
lines; local-day arithmetic in `timeline`), `crossterm` for the TUI's alt-screen
and raw mode (the Go code writes escape codes directly; `ratatui` is optional
and the current frame is simple enough to keep as plain `write!` of CRLF lines,
which also keeps the golden tests byte-comparable). No pango crate is needed;
tooltip markup is string formatting plus a three-character escape.

Module layout suggestion for the rewrite crate: `barview` and `barlayout` as
pure modules with the Go tests ported as-is; `waybarchip` as `render::waybar`
returning `WaybarOutput` (serde struct with `#[serde(skip_serializing_if)]` for
`tooltip`/`alt`); `render::polybar` and `render::tui` returning `String`; one
`render::loop` generic over a `Formatter` trait `{ fn render(&mut self, &Snapshot, now) -> Frame; fn next_refresh(&Snapshot, now) -> Option<Instant> }`;
`bottombar` as its own module owning `SlotPublisher` and `Lifecycle`;
`forensics::{diagnose,history,timeline}` depending on the core crate's
`history` and `statustune` modules rather than re-declaring rule ids.

Things to preserve byte-for-byte for the v1 cutover: the Waybar JSON key order
(`text, tooltip, class, alt`), the `pick` tab format, `timeline --json` envelope,
`history tail --json` events, the ready/pid/visible file formats, the slot file
naming, the `status: pid=` log grammar, and `-version` output.
