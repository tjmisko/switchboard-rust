# Survey 02 — Process Discovery and Lifecycle

Read-only survey of the Go implementation under `~/Projects/switchboard`
(worktrees ignored). Packages covered: `internal/discovery`, `internal/osproc`,
`internal/proc`, `internal/detect`, `internal/projectname`,
`internal/conformance`, plus the daemon wiring in `cmd/switchboard/main.go` that
turns "a pid appeared / a pid died" into session start and end. All paths below
are relative to `~/Projects/switchboard`.

---

## 1. Responsibilities

- **Enumerate the process table cheaply and repeatedly** (once per second by
  default, `cmd/switchboard/main.go:87`) and classify each *previously unseen*
  pid as a Claude Code session, a Codex session, or nothing
  (`internal/discovery/discovery.go:361-385`).
- **Decide what counts as an interactive agent session** purely from process
  metadata: comm, exe path, and argv. This is the single predicate `Classify`
  (`discovery.go:105-114`); the daemon never trusts hooks for existence (AP7 in
  `docs/rust-rewrite-requirements-v0.2.md`).
- **Tag non-navigable but real sessions**: `claude -p` / `--print` runs are
  discovered and marked `Headless` (`discovery.go:124-134`,
  `internal/state/state.go` field `Headless`).
- **Observe death, never infer it**: one `pidfd_open(2)` handle per tracked pid,
  polled in its own goroutine, firing `onDeath` exactly once
  (`internal/osproc/source_linux.go:68-122`).
- **Provide a neutral OS-process seam** (`osproc.Source`) so the same daemon
  compiles on macOS with a stub today and a libproc+kqueue backend later
  (`internal/osproc/osproc.go:56-67`, `docs/phase4/*.md`).
- **Read per-process facts from `/proc`**: comm, cmdline, exe, cwd, ppid, run
  state, controlling tty via fd 0/1/2, and peak RSS (`internal/proc/proc.go`).
- **Backstop the in-memory death watch** with a per-tick liveness poll so a
  daemon restart, SIGKILL, or failed `Watch` cannot leave a ghost session
  (`main.go:344-395`, `docs/session-lifecycle-hazards.md`).
- **Walk the ppid chain** to attribute a hook fired from a shell wrapper to the
  nearest tracked agent process, stopping at any agent process
  (`internal/rpc/rpc.go:1407-1422`).
- **Detect the backend stack at startup** (WM, terminal locator, OS process
  source) from environment variables and socket existence
  (`internal/detect/detect.go:37-109`).
- **Derive a project label from a cwd**: walk up to the git root, match a
  user or built-in abbreviation rule (`internal/projectname/projectname.go`).

---

## 2. Core types

### Domain types (carry meaning; must survive the rewrite)

| Type | Location | Notes |
|---|---|---|
| `osproc.Info` | `internal/osproc/osproc.go:26-34` | `{PID, PPID int; Comm, Exe, CWD, TTY string; Args []string}`. The neutral process record. `TTY` is an opaque join key; `Args` is nil when the kernel masks cmdline (zombies, kernel threads). |
| `osproc.Source` | `osproc.go:56-67` | `Enumerate() ([]Info, error)`, `Read(pid) (Info, error)`, `Watch(ctx, pid, onDeath func()) error`, `Stop(pid)`. The Seam-1 contract. |
| `osproc.ErrGone` | `osproc.go:47` | "Process disappeared between enumerate and read." Benign. |
| `osproc.ErrUnsupported` | `osproc.go:51` | Darwin stub answer. Must never be read as death (`main.go:344-353`). |
| `discovery.Agent` | `discovery.go:93-99` | `""` / `"claude"` / `"codex"`. Its string value is copied into `state.Session.Agent`. |
| `discovery.Classify` | `discovery.go:105-114` | The one predicate the scanner and every liveness check filter on. |
| `discovery.IsHeadless` | `discovery.go:124-134` | `-p`/`--print` anywhere after argv[0]. |
| `proc.Info` | `internal/proc/proc.go:12-21` | Same fields as `osproc.Info` plus `State string` (run-state char). |
| `proc.Suspended` | `proc.go:103-105` | `state == "T"` only. `"t"` (tracing stop) is deliberately not suspended. |
| `conformance.SourceFixture` / `RunSourceContract` | `internal/conformance/conformance.go:56-160` | The backend-agnostic contract, see §6. |
| `projectname.ProjectRule` / `Config` | `projectname.go:38-51` | Ordered rule list; first match wins; path overrides beat basename defaults. |
| `detect.Stack` / `detect.Options` | `detect.go:22-34` | The selected `{OSProc, Terminal, WM}` triple and the forcing knobs. |

### Plumbing types (Go-shaped; free to reshape)

| Type | Location | Notes |
|---|---|---|
| `discovery.Scanner` | `discovery.go:317-321` | `mu sync.Mutex; seen map[int]struct{}; src procSource`. The seen-set state machine. |
| `discovery.procSource` | `discovery.go:279-282` | Narrow test seam: `AllPIDs() + Read(pid)`. |
| `discovery.pidLister` | `discovery.go:289-291` | Optional-interface upgrade: a `Source` that can list pids without per-pid reads. |
| `discovery.osprocSource` | `discovery.go:298-315` | Adapter from `osproc.Source` to `procSource`; falls back to `Enumerate` when no `AllPIDs`. |
| `osproc.linuxSource` | `source_linux.go:28-31` | `mu sync.Mutex; watched map[int]context.CancelFunc`. |
| `osproc.darwinSource` | `source_darwin.go:13-21` | Every method returns `ErrUnsupported` / no-op. |
| `proc.Reader` | `proc.go:27-51` | Root-overridable `/proc` reader; zero value reads the real `/proc`. Only exists so parsers are testable against a fixture tree. |
| `osproc.FromProc` | `osproc.go:41-43` | Field-copy from `proc.Info` to `osproc.Info`. Exists because `rpc.findTrackedAncestor` still uses `proc.Read` directly. |
| `testsupport.Child` | `internal/testsupport/child.go:15-19` | Real `sleep` child with kill-and-reap for death tests. |

---

## 3. Concurrency

Every goroutine, lock, timer, and blocking syscall in this subsystem, with who
starts it and how it stops.

### 3.1 Goroutines

| # | Goroutine | Started by | Stops when | Talks to |
|---|---|---|---|---|
| G1 | Scanner loop `Scanner.Run` | `main.go:246-250`, `go func(){ scanner.Run(ctx, *scanInterval, onAgentAppeared) }` | `ctx.Done()` (SIGINT/SIGTERM via `signal.NotifyContext`, `main.go:112`) | Calls `onAgentAppeared` synchronously on the scanner goroutine, which does `resolver.Resolve` (forks `wezterm cli list` with a 3 s timeout, `mapping.go:64-70`), `store.Apply`, `sink.Record`, `procSrc.Watch`. |
| G2..Gn | One pidfd watcher per watched pid | `linuxSource.Watch`, `source_linux.go:87-120` | (a) pidfd becomes readable (death), fires `onDeath`, returns; (b) `Stop(pid)` cancels its child ctx, noticed within one 1 s poll tick; (c) parent ctx cancelled; (d) `poll` returns a non-EINTR error. Cleanup deletes itself from `watched` and closes the fd via `defer`. | `onDeath` closure from `main.go:236-243` runs **on the watcher goroutine** and does `store.Apply(endSession...)`, which records history and calls `forgetRoot` → `scanner.Forget` + `agentRuntime.RequestCleanup()`. |
| G-esrch | Already-dead fast path | `source_linux.go:78`, `go onDeath()` | Immediately | Same `onDeath` closure, fired from a fresh goroutine so `Watch` never runs the callback under `s.mu`. |
| G3 | Reconciler `runReconciler` | `main.go:254` | `ctx.Done()` | Per tick: `sessionDead(stack.OSProc, pid)` for every session (`main.go:952`), `proc.State(pid)` for suspension (`main.go:965`), then `store.Apply` closes dead lanes via `endSession` (`main.go:~985`). |
| G4 | WM loop `runWMLoop` | `main.go:253` | `ctx.Done()` | On `EventWindowClosed` calls `sessionDead` and `endSession` (hazard L7). Not part of this subsystem but consumes `osproc.Source`. |

Note that `sweepDeadSessions` (`main.go:369`) is a standalone function used by
the lifecycle tests; the live reconciler inlines the same logic at
`main.go:952` and in its `store.Apply` block. Both paths call `sessionDead`.

### 3.2 Locks

| Lock | Location | Guards | Held across |
|---|---|---|---|
| `Scanner.mu` | `discovery.go:318` | `seen` map | Only the map lookup/insert/delete. `onAppeared` is called with the lock released (`discovery.go:380-383`, pinned by `TestScannerCallbackIsLockFree`, `discovery_test.go:497-511`). |
| `linuxSource.mu` | `source_linux.go:29` | `watched` map | Held across `unix.PidfdOpen` in `Watch` (`source_linux.go:69-85`). A syscall under the lock, but a cheap one. |
| `Store` lock (external) | `internal/state` | session map | `endSession` requires the caller hold it (`main.go:305-319` comment). The pidfd `onDeath` closure and the reconciler both enter `store.Apply`, so death and sweep are serialised by the store. |

### 3.3 Timers, tickers, and blocking I/O

| What | Location | Cadence | Notes |
|---|---|---|---|
| `time.NewTicker(interval)` | `discovery.go:339` | `--scan-interval`, default 1 s (`main.go:87`) | First scan runs synchronously before the ticker (`discovery.go:341`). |
| `time.NewTicker(interval)` | `main.go:892` | `--reconcile-interval`, default 5 s (`main.go:88`) | Liveness backstop cadence. |
| `unix.Poll(pfd, 1000)` | `source_linux.go:104-105` | 1 s timeout per iteration | The only blocking syscall in this subsystem. Timeout exists solely so `Stop` and ctx cancellation are noticed; death itself is edge-triggered. |
| `os.ReadDir("/proc")` | `proc.go:144` | once per scan tick | One `getdents` over `/proc`. |
| `os.ReadFile` / `os.Readlink` on `/proc/<pid>/*` | `proc.go:60-82` | per unseen pid per tick; per session per reconcile tick | Up to 7 syscalls per `Read`: comm, cmdline, exe, cwd, status, fd/0, fd/1, fd/2 (short-circuits on the first pts). |

### 3.4 Channels

There are **no channels** inside `discovery`, `osproc`, or `proc`. Everything
is callback-based: `onAppeared(osproc.Info)` and `onDeath()`. The only channels
in the neighbourhood are in tests (`fired := make(chan struct{}, 4)`,
`source_linux_test.go:57`) and `serverErr` in main.

### 3.5 Data flow

```
                 ┌──────────────────────────────────────────────────────┐
                 │  /proc  (kernel)                                     │
                 │   readdir ──────────► AllPIDs()                      │
                 │   <pid>/{comm,cmdline,exe,cwd,status,fd/0..2} ─► Read│
                 │   pidfd_open(pid) ───► poll(POLLIN) ── readable on   │
                 │                                       zombie/exit    │
                 └───────┬───────────────────────┬──────────────────────┘
                         │                       │
        ┌────────────────▼──────────┐   ┌────────▼──────────────────────────┐
        │ G1 Scanner.Run (1 s tick) │   │ G2..Gn linuxSource.Watch goroutine │
        │  for pid in AllPIDs():    │   │  loop: poll(pidfd, 1000ms)         │
        │   if seen → skip          │   │   EINTR → continue                 │
        │   Read(pid)  (7 syscalls) │   │   n==0  → re-check ctx, continue   │
        │   Classify → None → skip  │   │   POLLIN|ERR|HUP|NVAL → onDeath()  │
        │   seen[pid]=∅             │   │  defer: close fd, delete watched   │
        │   onAppeared(info) ───────┼─┐ └────────┬──────────────────────────┘
        └───────────────────────────┘ │          │ onDeath closure (main.go:236)
                                      │          │  store.Apply(endSession)
     onAgentAppeared (main.go:200)    │          │    sink.Record(session_end)
      kind := Classify(info)  ◄───────┘          │    delete(m,pid)
      headless := IsHeadless(info)               │    forgetRoot(pid) ──► Scanner.Forget
      sess := resolver.Resolve(ctx, info)        │                     └► agentRuntime.RequestCleanup
        (wezterm/tmux Locate, WM findWindow)     │
      store.Apply(m[pid]=sess)                   │
      sink.Record(session_start)                 │
      procSrc.Watch(ctx, pid, onDeath) ──────────┘  (spawns G2..Gn; ESRCH → go onDeath())

        ┌───────────────────────────────────────────────────────────────┐
        │ G3 runReconciler (5 s tick)  — the durable liveness backstop  │
        │  for sess in Snapshot():                                      │
        │    dead := sessionDead(procSrc, pid)  (Read → ErrGone | !agent)│
        │    proc.State(pid) → Suspended                                │
        │  store.Apply: dead → endSession (same closure as above)       │
        └───────────────────────────────────────────────────────────────┘

        startup (before G1): dropStaleSessions(store, procSrc, …)   main.go:397
          Read every hydrated pid; definitively dead → endSession; alive → keep
```

---

## 4. External boundaries

### 4.1 `/proc` files read and how they are parsed

All reads go through `internal/proc/proc.go` unless noted.

| Path | Reader | Parse rule | Failure semantics |
|---|---|---|---|
| `/proc` (dir) | `AllPIDs`, `proc.go:143-160` | `os.ReadDir`; keep entries that are directories and whose name parses as `int`. | Error propagates; scanner skips that tick (`discovery.go:362-365`). |
| `/proc/<pid>/comm` | `Read`, `proc.go:60-64` | Whole file, `TrimRight "\n"`. Kernel truncates to 15 chars (`TASK_COMM_LEN` 16). | `ENOENT` → `ErrGone`; other errors propagate. **First read; gates the whole `Read`.** |
| `/proc/<pid>/cmdline` | `readArgs`, `proc.go:112-122` | Strip trailing NUL, split on NUL. Empty file (zombie, kernel thread) → `nil`. `[""]` → `nil`. | Never an error. Masked args mean `IsCodex` fails closed and `IsClaude` cannot detect the daemon verb. |
| `/proc/<pid>/exe` | `Read`, `proc.go:68-70` | `os.Readlink`. | Any error → `Exe = ""` (masked). Kernel masks exe for kernel threads and for processes you may not ptrace. |
| `/proc/<pid>/cwd` | `Read`, `proc.go:71-73` | `os.Readlink`. | Any error → `CWD = ""`. |
| `/proc/<pid>/status` | `Read`, `State`, `VmHWMKB` (`proc.go:75-82, 92-98, 225-231`) | Line scan. `PPid:` → `Atoi(TrimSpace)`, missing/non-numeric → 0 (`proc.go:181-189`). `State:` → first whitespace field, e.g. `"T (stopped)"` → `"T"` (`proc.go:194-205`). `VmHWM:` → first field as int64 kB, else 0 (`proc.go:237-254`). | `ENOENT` → `ErrGone`; other errors propagate. |
| `/proc/<pid>/fd/{0,1,2}` | `readTTY`, `proc.go:127-138` | `os.Readlink` each in order; return the first whose target has prefix `/dev/pts/`. | Errors skipped; none matching → `TTY = ""`. Note: this is "a std fd is a pty", not "the controlling tty" (`/proc/<pid>/stat` field 7 is never consulted). |

Other `/proc` readers outside the seam, listed so the rewrite does not miss them:

| Path | Reader | Purpose |
|---|---|---|
| `/proc/<pid>` (stat) | `internal/wezterm/wezterm.go:75` | Is the mux pid behind a `gui-sock-<pid>` still alive. |
| `kill(pid, 0)` | `internal/hyprland/hyprland.go:353-356` | Is the Hyprland instance pid alive; `EPERM` counts as alive. |
| `/proc/<pid>/comm`, `cmdline`, `stat` | `cmd/switchboard-ctl/bottombar.go:753, 855, 896, 910` | Waybar pidfile validation. `stat` field 22 (starttime, index 19 after the last `)`) is the pid-reuse fence (`bottombar.go:~915-930`). This is the **only** place the Go code uses process start time as identity. |
| `/proc/self/status` | `cmd/switchboard-ctl/seedbench.go:177` | Benchmark telemetry. |

### 4.2 pidfd usage

- `unix.PidfdOpen(pid, 0)` at `source_linux.go:74`. Requires Linux ≥ 5.3.
  `ESRCH` → already dead → `go onDeath()` and return nil without tracking
  (`source_linux.go:77-80`). Any other error (e.g. `EPERM`, `EMFILE`) is
  returned to the caller, which only logs it (`main.go:242-244`) and relies on
  the reconciler backstop.
- `unix.Poll([]PollFd{{Fd: pidfd, Events: POLLIN}}, 1000)` at
  `source_linux.go:104-105`. Death is any of `POLLIN | POLLERR | POLLHUP |
  POLLNVAL` (`source_linux.go:99`); the ERR/HUP/NVAL inclusion is the fix for
  decisions.md #11, where a `POLLERR` without `POLLIN` used to spin forever.
- `EINTR` → continue, never death (`source_linux.go:106-108`).
- `unix.Close(pidfd)` deferred on goroutine exit (`source_linux.go:88`).
- The pidfd is **never** used for `pidfd_send_signal` or `pidfd_getfd` in the
  daemon. `switchboard-ctl` does use `PidfdSendSignal` for Waybar
  (`bottombar.go:~800`).
- Nothing reads the pidfd's exit status or `waitid(P_PIDFD)`. Readability
  means "zombie or reaped", not "reaped", so a session whose parent shell has
  not yet waited still fires `onDeath`. That is intentional: the seam wants
  "no longer an interactive agent", not "fully reaped".

### 4.3 Signals

- `signal.NotifyContext(ctx, SIGINT, SIGTERM)` (`main.go:112`) is the only
  signal the daemon handles; it cancels every goroutine above.
- No `SIGCHLD` handling: the daemon never spawns the agents it watches, so it
  never reaps them. The `testsupport.Child.Kill` helper does `Kill` + `Wait` so
  a dead test child does not linger as a zombie in `/proc`
  (`testsupport/child.go:48-54`, comment at `conformance/source_test.go:50-52`).

### 4.4 Environment variables

| Variable | Where | Effect |
|---|---|---|
| `HYPRLAND_INSTANCE_SIGNATURE`, `XDG_RUNTIME_DIR` | `detect.go:77-80` via `wm.NewHyprland().Available()` | Hyprland wins if either the env or a live lock under `$XDG_RUNTIME_DIR/hypr/<sig>/hyprland.lock` exists (`hyprland.go:280-300`). |
| `SWAYSOCK`, `I3SOCK`, `DISPLAY` | `detect.go:81-90` | Precedence sway → i3 → X11 → none. |
| `XDG_CONFIG_HOME` | `projectname.go:292-298` | `…/switchboard/projects.json`. |
| `SWITCHBOARD_LIVE_CONFORMANCE` | `conformance.go:29` | Gates live WM/terminal assertions; the osproc contract is **not** gated (it spawns its own children). |
| `--scan-interval`, `--reconcile-interval`, `--wm`, `--terminal` | `main.go:87-90` | Flags, not env. |

### 4.5 Process identification rules

The predicate is `Classify(info)` (`discovery.go:105-114`): try `IsClaude`,
then `IsCodex`, else `AgentNone`.

| Provider | Rule | File:line | Accepts (example cmdline / fields) | Rejects |
|---|---|---|---|---|
| Claude | `Comm == "claude"` (exact, case-sensitive) | `discovery.go:43-45` | comm `claude` | comm `Claude`, `node`, `bash` |
| Claude | `Args[1] ∉ {"daemon", "mcp"}` | `discovery.go:30-33, 83-89` | `claude`, `claude --resume`, `claude "fix the build"`, `claude -p --model haiku` | `/home/u/.local/bin/claude daemon run --origin transient --spawned-by {...}` (the detached background daemon Claude Code reparents to init; shares comm+exe, no tty), `claude mcp serve` |
| Claude | exe valid for GOOS | `discovery.go:67-78` | Linux: `Exe == ""` (masked) or contains `/claude/` (`~/.local/share/claude/claude`, `~/.local/share/claude/versions/2.1.158`). Darwin additionally: `Exe == "claude"` or ends in `/claude` (`~/.local/bin/claude`, `/opt/homebrew/bin/claude`, `…/@anthropic-ai/claude-code/cli/claude`) | Linux: `/usr/local/bin/claude`, `/usr/bin/claude-impostor`; `/usr/local/bin/node` |
| Claude headless | any of `Args[1:]` is `-p` or `--print` | `discovery.go:124-134` | `claude -p …`, `claude --model haiku -p` | `claude`, `claude --resume` |
| Codex | `Comm == "codex"` | `discovery.go:191-193` | | |
| Codex | `Base(Args[0]) == "codex"` (args required) | `discovery.go:194-196` | `/usr/local/bin/codex`, `codex` | `codex-linux-sandbox --sandbox-policy-cwd /work/repo` (comm is `codex`!), `/tmp/impostor`, masked args, `[""]` |
| Codex | `Exe == "" \|\| Base(Exe) == "codex"` | `discovery.go:197-199` | masked exe, `/opt/codex/bin/codex` | `/usr/local/libexec/codex-linux-sandbox`, `/tmp/codex-impostor` |
| Codex | `codexIsInteractive(Args)`: walk global options; `--` → TUI; help/version → no; known value option must have a non-flag value; unknown `-x`/`--x` → no; `resume`/`fork` → yes unless help/version before `--`; known non-interactive verb → no; other bare token → yes **only if it contains whitespace** (a quoted prompt); end of argv → yes | `discovery.go:215-273` | `codex`, `codex --model gpt-5-codex --profile work --search`, `codex -c model=x resume thread-id`, `codex --config=… fork id`, `codex "fix the build"`, `codex -- exec`, `codex --`, `codex --full-auto` | `codex exec`, `codex app-server daemon`, `codex --help`, `codex resume --help`, `codex future-command`, `codex --future-mode`, `codex --model` (missing value), `codex -m --search`, `codex --profile=`, `codex -mgpt-5-codex` (attached short value) |

Tables of Codex vocabulary: non-interactive verbs `discovery.go:140-150`, value
options `156-167`, boolean flags `170-176`, help/version `178-180`. The
`codex-linux-sandbox` case is the documented regression that motivated the
identity checks (`docs/codex-session-status/01-discovery-hardening.md:24-33`).

Wrapper scripts: Claude may be launched through
`~/.config/scripts/claude-name-wrapper.sh`, which only prefixes `-n <name>` and
then execs the binary (`docs/session-naming.md:37-40, 121`). Because the shell
`exec`s, the observed process is still comm `claude`. The `findTrackedAncestor`
walk (`rpc.go:1407-1422`) exists for the case where a *hook* runs under a shell
wrapper, so `getppid()` is the wrapper, not the agent; it walks up at most 20
levels, returns 0 on any read error, and refuses to walk *past* an agent
process, so a nested `claude -p` that discovery has not yet seen cannot steal
its parent's identity (A2 race, `rpc.go:1377-1406`). Codex is never wrapped:
`docs/codex-no-wrapper-binding-probe.md:9-27` makes "ordinary unmodified
`codex`" a hard requirement and forbids cwd/title/time/launch-order joins.

Node vs native: since Claude Code v2.1.113 the CLI is a native signed binary,
so comm is `claude` not `node` (`discovery.go:37-38`,
`docs/phase4/05-discovery-and-testsupport.md:9`). The "comm `node` with
`claude` in argv" fallback is explicitly *not* built
(`05-discovery-and-testsupport.md:91-97`).

---

## 5. Dependencies

### Inbound (who uses this subsystem)

| Consumer | Uses | Location |
|---|---|---|
| `cmd/switchboard/main.go` | `discovery.New/Run/Forget/Classify/IsHeadless`, `osproc.Source.Read/Watch`, `proc.State/Suspended`, `detect.Detect`, `projectname.Load/CanonicalForDir` | `main.go:123-125, 148-155, 192-250, 344-460, 952-966` |
| `internal/rpc` | `proc.Read` (injected), `discovery.Classify`, `osproc.FromProc` | `rpc.go:260, 1407-1422` |
| `internal/mapping` | `osproc.Info` as `Resolve` input | `mapping.go:57` |
| `internal/state` | `discovery.Agent` string values; `Headless` semantics | `state.go` (`AgentKindClaude/Codex`, `Headless`) |
| `internal/state/publishstats.go`, `internal/fanout/seedtelemetry.go` | `proc.VmHWMKB` | telemetry lines |
| `internal/conformance` (tests) | `osproc.New`, `proc.Read(2)` for the masked-exe fixture | `conformance/source_test.go:78-95` |
| `cmd/switchboard-ctl` | `projectname` (name resolve/edit), raw `/proc` for Waybar | `switchboard-ctl/main.go:817-840`, `bottombar.go` |

### Outbound (what this subsystem depends on)

| Package | Depends on | Why |
|---|---|---|
| `discovery` | `osproc` only (`runtime.GOOS`, `path/filepath`, `strings`, `unicode`) | Pure classification over `osproc.Info`. |
| `osproc` | `proc`, `golang.org/x/sys/unix` v0.27.0 (Linux only) | `/proc` reads and `pidfd_open`/`poll`. |
| `proc` | stdlib only; **no build tags** | Compiles on darwin so telemetry callers link; returns errors there. |
| `detect` | `osproc`, `state`, `terminal`, `wm` | Builds the stack. |
| `projectname` | stdlib (`os`, `encoding/json`) | Reads `.git` presence and `projects.json`. |
| `conformance` | stdlib + `testing` | Deliberately imports none of the backends. |

---

## 6. Invariants and hazards

### 6.1 Contracts the conformance suite requires (`conformance.go:77-160`)

1. `Enumerate` includes an interactive child with **non-empty** `TTY` and
   **non-empty** `CWD`. Asserts non-emptiness only, never `/dev/pts/`
   (`conformance.go:80-103`).
2. A child with no controlling terminal reads back with `TTY == ""`
   (`conformance.go:105-114`).
3. `Read` of a killed-and-reaped pid returns the gone sentinel
   (`conformance.go:116-122`).
4. A pid whose exe/cwd are unobtainable (kthreadd, pid 2) reads with **no
   error** and `Exe == ""` (`conformance.go:124-136`; fixture at
   `source_test.go:88-94`).
5. `Watch` fires `onDeath` exactly once within 3 s of death and not again
   within a further 300 ms (`conformance.go:138-159`).

Linux-specific additions in `source_linux_test.go`:

- `TestWatchFiresOnDeathExactlyOnce` and `Watched()` drains within 3 s (no
  goroutine leak, `source_linux_test.go:51-78`).
- `TestDuplicateWatchIsNoOp`: second `Watch` for a live pid adds no watcher and
  never fires a second callback (`82-112`).
- `TestStopCancelsWithoutFiringOnDeath` (`115-135`).
- `TestWatchAlreadyDeadFiresImmediately`: `DeadPID()` (0x40000000, above
  `pid_max`) fires immediately and never enters `Watched()` (`140-158`).
- Documented-not-tested: EINTR must not fire; POLLERR-without-POLLIN is now
  death (header comment `source_linux_test.go:13-22`, `docs/decisions.md` #11).

Scanner invariants (`discovery_test.go`):

- Fires once per pid; `Forget` re-fires (`380-398`).
- An errored `Read` is not remembered (`402-420`); a non-agent pid is not
  remembered (`424-441`); a rejected Codex sandbox pid is not remembered, so a
  later real TUI on the same pid is still found (`447-476`).
- ⚠ Characterisation: a recycled pid **without** a `Forget` is shadowed forever
  (`481-493`, `docs/decisions.md` #12).
- Callback runs lock-free (`497-511`).
- Both adapter paths (fast `AllPIDs` and `Enumerate` fallback) classify the same
  (`555-589`).

Lifecycle invariants (`cmd/switchboard/session_lifecycle_test.go:79-160` and
`docs/session-lifecycle-hazards.md` §5):

| Id | Scenario | Required behaviour |
|---|---|---|
| L1-restart-orphans-watch | daemon restarted; watch lost; process later dies | reconciler `sessionDead` → `ErrGone` → `endSession` within one 5 s tick |
| L2-death-during-downtime | died while daemon down; hydrated state still has it | `dropStaleSessions` records `session_end` *before* deleting (`main.go:397-445`) |
| L3-watch-registration-failed | `Watch` returned error at discovery | same sweep closes it |
| L4-live-idle-not-a-ghost | alive but idle for hours | **never** ended; liveness keys on pid death, not inactivity |
| L4b-recycled-pid-is-a-death | pid now belongs to `bash` | `Classify == AgentNone` → definitive death (FR-A5) |
| L4c-unreadable-is-not-a-death | `ErrUnsupported` or transient error | do nothing; retry next tick (three-valued liveness, AP9) |
| L5-no-double-end | sweep and late pidfd callback both see it | one `session_end`; dedup is map membership under the store lock (`main.go:320-333`) |
| L7-window-closed-bare-delete | WM says window closed | route through `endSession` only if `sessionDead`; otherwise keep tracking |

### 6.2 Hazards to carry into the rewrite

- **PID reuse.** `Session.StartedAt` is discovery wall-clock time
  (`mapping.go:62`), and is explicitly *not* a kernel birth token
  (`main.go:207-215`). Provider roots are keyed `(pid, StartedAt)`
  (`agent_observation.go:198`) and every action re-checks `StartedAt` equality
  (`main.go:~975`). The only reader of `/proc/<pid>/stat` starttime is the
  Waybar pidfile fence in `switchboard-ctl`. Recycling between `Read` and
  `PidfdOpen` is acknowledged and accepted (`docs/phase4/02:232`). A rewrite
  should carry starttime (from `stat` field 22 or `pidfd_getinfo`) in the
  process record so `(pid, starttime)` is the identity everywhere.
- **Death vs scan race.** A pid can be enumerated, then vanish before `Read`:
  `ErrGone` is benign and the pid is *not* remembered
  (`discovery.go:373-376`). A pid can also die between `Read` and `Watch`:
  `PidfdOpen` → `ESRCH` → immediate `onDeath` (`source_linux.go:77-80`), which
  then runs `endSession` on a session that `store.Apply` at `main.go:222`
  already inserted, so the lane opens and closes correctly. If the `onDeath`
  goroutine won the race against that `store.Apply`, `endSession` would find no
  session and the later insert would leave a permanently untracked-by-watch
  session until the sweep. This ordering is guaranteed only because `Watch` is
  called *after* `store.Apply` on the same goroutine.
- **Zombies.** A zombie keeps its `/proc` entry with an empty `cmdline`, so
  `Read` succeeds with `Args == nil` and comm intact. For Claude that means a
  zombie `claude` (parent has not waited) still passes `IsClaude` (masked exe
  accepted, no daemon verb detectable) and would be *re-discovered* after a
  `Forget`. The pidfd fires on zombie transition, so `endSession` → `Forget`
  → next scan sees the zombie → `onAppeared` again → `Watch` on a zombie:
  `PidfdOpen` on a zombie succeeds and `poll` returns readable immediately, so
  the loop is self-limiting but noisy. Nothing in the Go code checks `State ==
  "Z"` in the classifier. **Recommendation:** reject `State == "Z"` in
  classification.
- **Sessions that restart / resume.** A `claude --resume` in a new pane is a
  new pid and a new session; the same `session_id` then spans two lanes
  (`session-lifecycle-hazards.md` §2.1, F3). Discovery is pid-keyed; identity
  continuity is a history-reader concern.
- **cwd changes.** `CWD` is captured once at discovery (`mapping.go:60`). The
  reconciler re-resolves terminal/WM but never re-reads `/proc/<pid>/cwd`. A
  Claude session that `cd`s (via a shell tool) keeps its original cwd label.
  This is arguably correct (the project is where it started).
- **ptrace / permission failures.** `exe` and `cwd` readlinks require
  `PTRACE_MODE_READ`; for same-uid processes they succeed, for others they
  fail and are silently masked. `PidfdOpen` on another user's pid succeeds but
  `poll` still works; `kill(pid,0)` returning `EPERM` is treated as alive in
  `hyprland.go:353`. Masked exe is accepted for Claude (comm gate already
  matched) but a masked `Args` fails closed for Codex.
- **Containers / pid namespaces.** The daemon only sees its own pid namespace.
  Agents in a container (rootless podman/docker) are invisible unless the
  daemon shares the namespace. Nothing handles `/proc/<pid>/status NSpid`.
  No code path addresses this; it is an accepted limitation.
- **`comm` truncation.** 15 chars on Linux, 16 on macOS; `claude`/`codex` fit,
  so exact match is safe but must never be tightened to a longer string
  (`docs/phase4/05:21`).
- **Watch fd budget.** One pidfd + one goroutine per session. Fine for tens of
  sessions; `Watch` returning `EMFILE` is only logged.
- **`Watch` does not verify identity.** It opens whatever pid it is given. If
  the scanner's `Read` and the `PidfdOpen` straddle a recycle, the watch tracks
  the wrong process and `endSession` fires when *that* dies. The sweep's
  `Classify == AgentNone` check is what repairs it.
- **Darwin stub collapses everything to `ErrUnsupported`.** `sessionDead`
  must keep treating that as "unknown" (`main.go:344-353`); a rewrite that
  makes liveness a `bool` will regress L4c on the first non-Linux backend.

---

## 7. Essential vs accidental

### Essential (intrinsic to the problem)

- The three-valued liveness answer: alive / gone / unknown.
- Death observed from a kernel handle, with a polled backstop that repairs a
  lost handle. Both are needed: pidfd for latency, poll for durability.
- Exactly-once `session_end`, deduplicated by store membership under one lock.
- A pure classifier over `{comm, exe, argv}` with the exact Claude and Codex
  rules above, including fail-closed for unknown Codex verbs and the
  `codex-linux-sandbox` rejection.
- Headless tagging from argv.
- The neutral `Info` record with masked-field semantics ("empty, not error").
- The conformance contract: it is the acceptance test for any backend and is
  worth porting verbatim as a Rust test harness generic over the trait.
- The identity walk stopping at any agent process (A2).
- Project-root walk and abbreviation rules.

### Accidental / Go-shaped / historical

- **`internal/proc` vs `internal/osproc` split.** Two packages exist because
  `procwatch` was absorbed later (Phase 1.1) and `rpc.findTrackedAncestor` still
  reads `proc.Info`; `osproc.FromProc` is glue. One process module with a
  `Linux` backend is enough.
- **`procSource` + `pidLister` + `osprocSource` adapter chain**
  (`discovery.go:279-315`). This is an optional-interface trick to keep an
  injectable fake while allowing a cheap pid list. In Rust: one trait with
  `list_pids()` as a required method (a backend without a cheap list can
  implement it via enumerate).
- **Seen-set inside the scanner + `Forget` from the death path.** The seen set
  exists because `Read` is 7 syscalls and the scan is 1 Hz over ~300 pids. It
  is *state duplicated from the session store* (a pid is "seen" iff it is a
  tracked session), and that duplication is the root of decisions.md #12. A
  rewrite can key "unseen" off the session map itself, or off
  `(pid, starttime)`, and drop `Forget`.
- **1 s `poll` timeout to notice cancellation.** A Go-ism for interruptible
  blocking syscalls. Rust async on a pidfd (`AsyncFd` + `select!` on a cancel
  token) removes the tick entirely.
- **One goroutine per watched pid.** Not a problem at this scale but
  unnecessary with an event loop; epoll over all pidfds is one task.
- **Callback style** (`onAppeared`, `onDeath`) runs domain logic (store
  writes, history, terminal forks) on the scanner and watcher goroutines. In
  Rust, emit events (`Appeared(Info)`, `Died(pid)`) on a channel and let the
  core own the state.
- **`proc.Reader` root override** exists only to unit-test parsers against a
  fake `/proc` tree. Parsers that take `&str` need no such indirection.
- **`claudeExeValid` parameterised on GOOS** with a Linux-tighter rule. The
  tightness is defensive against `/usr/local/bin/claude-impostor`; the
  basename rule alone already rejects that. A single rule ("basename is
  `claude` or path contains `/claude/`") is sufficient on both.
- **`VmHWMKB` living in `proc`.** Telemetry; unrelated to discovery.
- **`sweepDeadSessions` as a dead function** with the logic inlined into
  `reconcileOnce`. Keep one.
- **`Args` field added late** to the seam (for the `claude daemon` filter).
  Intrinsic now.
- **Reading fd 0/1/2 for the tty.** The controlling terminal is available in
  `/proc/<pid>/stat` field 7 (`tty_nr`) as a `dev_t`, resolvable via
  `/dev/pts/<minor>` for the pts major. The fd-walk works and matches
  "interactive TUI", but a Rust backend could read `stat` once for
  `tty_nr`, `ppid`, `state`, and `starttime` in a single syscall.

---

## 8. Rust mapping sketch

### Modules

```
switchboard-rust/src/
  proc/
    mod.rs        pub trait ProcessSource; pub struct ProcessInfo; pub enum Liveness
    linux.rs      /proc parsing (pure fns over &str) + LinuxSource
    linux_pidfd.rs  DeathWatch on pidfd via tokio::io::unix::AsyncFd
    darwin.rs     stub returning Unsupported (later libproc + kqueue)
    fake.rs       in-memory source for tests (cfg(test) or a feature)
  discovery/
    classify.rs   Agent enum, is_claude, is_codex, is_headless, codex argv parser
    scanner.rs    tick loop → Appeared events
  lifecycle/
    mod.rs        end_session, session_dead, sweep, startup stale-drop
  detect.rs       Stack selection from env
  projectname.rs  rules, root walk, config file
  conformance/    test-only generic harness over ProcessSource
```

### Types

```rust
pub struct Pid(pub i32);

/// Kernel identity: pid + starttime (clock ticks since boot, /proc/<pid>/stat f22).
/// Equality here is what "same process" means everywhere in the daemon.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProcId { pub pid: Pid, pub start_ticks: u64 }

pub struct ProcessInfo {
    pub id: ProcId,
    pub ppid: Pid,
    pub comm: String,
    pub exe: Option<PathBuf>,      // None == masked (never an error)
    pub cwd: Option<PathBuf>,
    pub tty: Option<String>,       // opaque join key
    pub args: Option<Vec<OsString>>, // None == masked (zombie, kthread)
    pub state: RunState,           // R S D T t Z X ...
}

#[derive(Debug)]
pub enum ReadError { Gone, Unsupported, Io(std::io::Error) }

/// Three-valued liveness; never collapse Unsupported into Gone (L4c, AP9).
pub enum Liveness { Alive(ProcessInfo), Gone, Unknown(ReadError) }

pub trait ProcessSource: Send + Sync {
    fn list_pids(&self) -> Result<Vec<Pid>, ReadError>;
    fn read(&self, pid: Pid) -> Result<ProcessInfo, ReadError>;
    fn enumerate(&self) -> Result<Vec<ProcessInfo>, ReadError> { /* default over list+read */ }
    /// Resolves exactly once when the process exits, however it exits.
    /// Err(Gone) if already dead at registration.
    fn watch(&self, id: ProcId) -> Result<DeathHandle, ReadError>;
}

/// A future that completes on death. Dropping it cancels the watch (== Stop).
pub struct DeathHandle(/* AsyncFd<OwnedFd> on Linux */);
impl Future for DeathHandle { type Output = (); ... }

pub enum Agent { Claude, Codex }
pub struct Classified { pub agent: Agent, pub headless: bool }
pub fn classify(info: &ProcessInfo) -> Option<Classified>;

pub enum DiscoveryEvent { Appeared(ProcessInfo, Classified), Died(ProcId) }
```

### Crates

| Concern | Crate | Notes |
|---|---|---|
| pidfd_open, poll, kill, pty for tests | `rustix` (preferred) or `nix` | `rustix::process::pidfd_open`, `rustix::event::poll`. Avoid pulling `libc` directly. |
| Async readiness on the pidfd | `tokio` `io::unix::AsyncFd<OwnedFd>` | `readable().await` completes on exit. One task per watch is fine, or a single epoll task; either way no 1 s tick. |
| `/proc` parsing | hand-rolled over `std::fs` | `procfs` crate is heavier than needed and pulls in a lot; the parsers here are five lines each. Read `stat` once for ppid/state/tty_nr/starttime; keep `comm`, `cmdline`, `exe`, `cwd` readlinks. |
| Cancellation | `tokio_util::sync::CancellationToken` | Replaces `context.Context`. |
| Errors | `thiserror` | `ReadError` with `#[error]` variants. |
| macOS later | `libproc` crate (safe wrappers over `proc_pidpath`/`proc_pidinfo`) + `rustix`/`nix` kqueue | Same trait; conformance harness runs unchanged. |

### Sync vs async

- `list_pids`, `read`, `enumerate`: **sync**. They are a handful of small file
  reads and are called from a periodic task; wrap the scan in
  `tokio::task::spawn_blocking` if it ever shows up in a profile.
- `watch`: returns an **async** handle. The core does
  `select! { _ = handle => Died(id), _ = cancel.cancelled() => {} }`.
- The scanner is one `tokio::time::interval` task that emits
  `DiscoveryEvent::Appeared` on an `mpsc` channel; the lifecycle core owns the
  session map and is the only writer of `session_end`, on one task, so the
  L5 dedup is by construction rather than by lock discipline.
- The reconciler backstop calls `source.read(pid)` per session per tick and
  maps `Err(Gone)` or `Ok(info) if classify(&info).is_none()` to death, and
  everything else to "leave alone", exactly `sessionDead` (`main.go:344-353`).
- Use `ProcId` (pid + starttime) as the session key; then a recycled pid is a
  different key, the seen-set collapses into "is this ProcId in the map", and
  `Forget` disappears.

### Tests to port first

1. `TestIsClaude`, `TestClaudeExeValid`, `TestIsCodex`,
   `TestIsCodexRejectsTopLevelCommands`, `TestIsCodexRejectsAppServerForms`,
   `TestIsCodexRejectsHelpAndVersion`, `TestClassify`, `TestIsHeadless`
   (`discovery_test.go:36-376, 591-612`): pure tables, port verbatim.
2. `TestParsePPID`, `TestParseState`, `TestSuspended`, `TestParseVmHWMKB`
   (`proc_test.go:13-73, 133-156`) plus a new `parse_stat` table for starttime
   and tty_nr.
3. The scanner seen-set tests (`discovery_test.go:380-589`) against the fake
   source.
4. `RunSourceContract` as a generic `fn source_contract<S: ProcessSource>(…)`
   with a `SpawnTTYChild` helper (`/dev/ptmx` + `TIOCSPTLCK` + `TIOCGPTN`,
   `testsupport/child_linux.go:22-60`) and the four Linux death tests
   (`source_linux_test.go`).
5. `TestSessionLifecycleHazards` rows L1/L3/L4/L4b/L4c and
   `TestEndSessionEmitsExactlyOncePerDeath` (`session_lifecycle_test.go:79-233`).
