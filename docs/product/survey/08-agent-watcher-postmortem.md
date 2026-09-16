# 08 — agent-watcher post-mortem and salvage assessment

Read-only survey of `~/Projects/agent-watcher`, the agent-built Rust rewrite of the
Go daemon, taken 2026-09-10. All paths below are relative to that repo unless
prefixed. Nothing there was modified. Verification commands run: `cargo build
--workspace`, `cargo test --workspace`, `cargo tree`.

**Bottom line.** It builds and its tests pass, and a Linux vertical slice ran live
once. But ~40% of the Rust is tests, ~6% is blind macOS/Windows code that never
executed, ~15% is a conformance suite for backends that do not exist, and the two
subsystems the design brags about most (pidfd death watch, task supervisor) are not
wired into the binary. The status model it ported has since been superseded on the
Go side. Salvage is a handful of parsers, syscall wrappers, fixtures and rule data,
maybe 1,500 lines, not the architecture.

---

## 1. What it is

Cargo workspace, 15 crates, 26 commits from 2026-08-12 to 2026-08-14 (plus one
un-integrated port on 2026-08-21 that never got its own commit, see §2).

### Crate map

| Crate | Rust LOC | Non-test LOC | `#[test]`s | Role |
|---|---:|---:|---:|---|
| `aw-core` | 3,096 | 2,069 | 53 | Pure types, per-writer FSM (`writer.rs`), colour fold (`fold.rs`), Codex slot reducer (`codex_slot.rs`). No I/O. |
| `aw-proc` | 9,190 | 6,927 | 135 | Seam 1: `/proc` reader, `pidfd` death watch, memory sampling. Includes **blind** `macos/` (2,268) and `windows/` (1,938). |
| `aw-discovery` | 2,614 | 1,434 | 47 | Which pids are agents: TOML match rules + argv "program position" logic. |
| `aw-terminal` | 4,801 | 1,371 | 69 | Seam 2: tty → pane. WezTerm CLI, tmux, and a tmux+wezterm chain. |
| `aw-wm` | 4,833 | 2,850 | 62 | Seam 3: Hyprland IPC (`hyprland.rs`) + the pane→window join (`join.rs`). |
| `aw-agent` | 4,216 | 1,962 | 83 | Claude/Codex hook parsing, transcript readers, `SessionLog` trait, hook forwarder. |
| `aw-wire` | 3,314 | 1,091 | 48 | v2 JSON schema, hand-written `Serialize`, byte-level goldens. |
| `aw-rpc` | 1,119 | 756 | 10 | Control-socket request/response enums + async client. |
| `aw-daemon` | 12,597 | 8,271 | 160 | The binary: host/tick loop, `Core` state, hook socket, RPC server, decision log, `diagnose`, supervisor. |
| `aw-ctl` | 2,106 | 1,887 | 47 | CLI (`list`, `focus`, `status`, `diagnose`, `hook`). clap. |
| `aw-waybar` | 1,645 | 975 | 38 | Waybar renderer client. |
| `aw-tui` | 2,109 | 1,197 | 48 | ratatui TUI client. |
| `aw-timeline` | 611 | 384 | 7 | serde port of the dashboard timeline envelope. |
| `aw-conformance` | 9,833 | 8,673 | 46 | Trait-contract suites for proc/terminal/wm/codex-slot backends + hostile fakes. |
| `aw-replay` | 5,481 | 4,136 | 46 | Trace-replay oracle over `traces/`, stub reducers as negative controls. |
| **Total** | **67,565** | **43,900** | **899** | |

"Non-test" excludes everything after the first `#[cfg(test)]` in a file and the
`tests/` directories (23.7k lines of tests total). Markdown alongside: **18,923
lines** across `docs/`, `session-notes/`, `reviews/`, `traces/*.md`, `CLAUDE.md`,
`todo.md`, `DONE.md`.

### Crate dependency graph

```
aw-core ──┬── aw-proc ──┬── aw-discovery ──┐
          │             ├── aw-terminal ── aw-wm ──┤
          ├── aw-agent ──────────────────────────┤
          ├── aw-wire ── aw-rpc ── {aw-ctl, aw-waybar, aw-tui}
          │                  └───────────────────┴── aw-daemon
          ├── aw-replay
          └── aw-conformance (← aw-proc, aw-terminal, aw-wm)
aw-timeline (serde only, deliberately not on aw-core)
```

### External dependencies (from `cargo tree --workspace`, direct)

| Crate | Version | Used by |
|---|---|---|
| `serde` | 1.0.229 | nearly all |
| `serde_json` | 1.0.151 | agent, daemon, replay, rpc, wire, wm, ctl, waybar |
| `thiserror` | 2.0.20 | all libraries |
| `tokio` | 1.53.1 | daemon (`rt-multi-thread, macros, signal, sync, time, net, fs, io-util`), terminal (`process, time`), wm (`net, time, io-util`), rpc, ctl, waybar, tui |
| `toml` | 0.9.12 | daemon, discovery |
| `tracing` / `tracing-subscriber` | 0.1.44 / 0.3.23 (`env-filter`) | daemon, agent |
| `rustix` | 1.1.4 (`process, event`) | proc (Linux only) |
| `clap` | 4.6.6 (`derive`) | ctl |
| `anyhow` | 1.0.104 | ctl |
| `ratatui` / `crossterm` | 0.30.2 / 0.29.0 | tui |
| `libc` | 0.2 | proc, macOS target only |
| `windows-sys` | 0.61.2 | proc, Windows target only |

Transitive closure on Linux: `cargo tree --workspace --edges normal` prints ~130
distinct packages; `aw-daemon` alone pulls ~65, `aw-core` 13. The heavy tails are
`ratatui` (kasuari, unicode-*, strum, compact_str) and `clap`/`tracing-subscriber`
(regex-automata, matchers). The daemon itself is not heavy; the clients are.

---

## 2. Where it got to

**Does it build today?** Yes. `cargo build --workspace`: clean in 4.7 s with two
warnings (`main.rs:52` unused import, `diagnose.rs:788` redundant pattern).
`cargo test --workspace`: **893 passed, 0 failed, 1 ignored** across 78 test
binaries (measured 2026-09-10).

### Milestones as recorded in `DONE.md`

| Milestone | Status | Evidence |
|---|---|---|
| Phase A: traces, spikes, schema draft, fork-point spike | landed 2026-08-12 | `docs/spikes/*`, `docs/v2-schema.md`, `traces/` (8 timing + 6 lifecycle jsonl) |
| M0 skeleton + M1 Linux process layer | landed, **ran live** 60 s | `docs/spine-status.md` §"The live run" |
| M2 terminal seam + M3 Hyprland + four-hop join | landed, ran live (3 sessions joined tty→pane→window) | `session-notes/2026-08-12 - seams.md` |
| M4 v2 wire contract | landed, byte-golden pinned | `crates/aw-wire/tests/` |
| M5 hooks + transcript | wired, `agent.status` published live; **H1–H9 replay gate not met** | `session-notes/2026-08-12 - status.md`; `todo.md` #18 still open |
| M6 RPC + `aw-ctl` + renderers | landed, ran live | `session-notes/2026-08-13 - m6.md` |
| Decision log + `diagnose` | landed | `DONE.md` "FR-G3c" |
| M7 macOS, M8 Windows | **blind**: typechecks under cross-compile, never executed | `session-notes/2026-08-13 - m7-m8-blind.md` |
| Codex slot/conversation port | 2026-08-21, in `DONE.md` but **no commit**: `git log` ends 2026-08-14 | `session-notes/2026-08-21 - codex slots.md` |

13 of 23 `todo.md` tasks closed. The Linux slice is real: discovery → join → status
→ file + RPC → waybar/TUI.

### What ran live vs. what is only proven by test

Ran live on this machine: enumeration, discovery filter (390 pids → 3 sessions),
Hyprland client listing, WezTerm pane resolution, the file publisher, the RPC bus,
`aw-ctl list`, `aw-waybar`, hook forwarder exit path. Verified as no-op against the
Go daemon (distinct paths and sockets).

Never observed live, per the project's own notes (`docs/spine-status.md` §"Not
proven live", `session-notes/2026-08-13 - status report.md`):

- **A real `PermissionRequest` end to end.** Everything RED is proven only by unit
  tests and replay corpus. This is the daemon's whole purpose.
- **Focus actuation.** Gated behind `AW_M6_ALLOW_FOCUS`, never flipped.
- **The live WezTerm certification** silently skips under `cargo test` because no
  agent run has a controlling tty (`status report` §"one place it did not").
- Eleven hazard traces (H4, H5, H9-input, H9b, L1, L2, L3, L4b, L4c, L5, L7) were
  never captured because they need a human at a keyboard.

### Why work stopped

The last real commit is 2026-08-14 (`chore: adopt taskpump`); three `tasks/T*.md`
stubs were created and never claimed. The 2026-08-13 status report lists the
blockers in order: (1) an **owner decision** on the fork point that was "supposed to
clear M4" and never came, (2) traces needing a human, (3) hardware for M7/M8. Every
remaining item was one only the owner could unblock, and the agents had exhausted
what they could do alone, which is how the blind platform backends and the 4.5k
lines of macOS/Windows research got written. The Go side then moved on: the
`agentgraph` package (`~/Projects/switchboard/internal/agentgraph`) replaced the
writer FSM the Rust port was built around, so the oracle the port was measured
against no longer exists.

---

## 3. Concurrency architecture actually used

Read `crates/aw-daemon/src/main.rs:110-320` and `host.rs` for what the binary does,
not the module docs, because they diverge.

**Runtime.** `#[tokio::main(flavor = "multi_thread")]` (`main.rs:88`). Features:
`rt-multi-thread, macros, signal, sync, time, net, fs, io-util`.

**Tasks spawned by `main`** (`main.rs:170-296`):

1. `host::install_signal_handlers` — SIGTERM/SIGINT into a `watch`-backed
   `Cancellation` (`host.rs:114-166`).
2. `hook_socket::serve` — Unix listener, one short task per connection, 0600 socket
   in a 0700 dir, payload cap, per-connection deadline (`hook_socket.rs`).
3. `rpc_server::serve` — Unix listener, `tokio::spawn` per connection
   (`rpc_server.rs:641-669`), plus a `push_stream` task per subscriber fed from a
   `watch::Receiver<Option<Arc<PublishedSnapshot>>>` (`rpc_server.rs:913`).
4. `Host::run` on the main task: `tokio::time::interval` with
   `MissedTickBehavior::Skip`, `select! { biased; cancelled, tick }`
   (`host.rs:925-950`).
5. `spawn_blocking` for the temp-file-plus-rename publish (`host.rs:431`).

**Channels.** `tokio::sync::watch` for cancellation and the snapshot bus; a
hand-rolled bounded drop-oldest `EventQueue` (`std::sync::Mutex<VecDeque>` +
`Notify`, `host.rs:240-300`) for producer events. **Its producer half is never
connected**: `main.rs:298` reads `let _events = events; // The producer half belongs
to the death-watch reactor, which lands next.` It never landed.

**State lock.** One `std::sync::Mutex<Core>` wrapped in `StateLock`
(`host.rs:499-530`) with a thread-local "held" flag. The `!Send` guard makes
holding it across `.await` a compile error; a test asserts `Host::run` is `Send`.
Tick shape: gather (unlocked, `/proc` sweep + transcript reads + seam joins) →
apply (locked, pure) → publish (unlocked). This part is sound and cheap.

**Death detection, as built.** `Host::gather` (`host.rs:1110-1236`) enumerates
`/proc`, and any known pid missing from the sweep is re-read individually; only
`ReadOutcome::Gone` (ENOENT/ESRCH on `/proc/<pid>/stat`) ends a session. That is a
polled kernel observation once per tick. The 1,936-line `deathwatch.rs` (own
`std::thread` reactor parked in `poll(2)` over pidfds with a self-pipe and std
`mpsc`, `deathwatch.rs:395-480`) and the `PidfdDeathWatch` in
`linux/mod.rs:368` are **not called from the daemon**; `rg watch_death
crates/aw-daemon/src` hits only test doubles. The Hyprland `.socket2` focus
subscription (`hyprland.rs:649-760`) is likewise implemented and unconsumed.

**Supervision.** `supervise.rs` (1,249 lines: exponential jittered backoff,
`TaskHealth`, `flock` single-instance lock) is **not referenced by `main.rs`**. The
three long-lived tasks are plain `tokio::spawn` handles that get `.abort()`ed on
exit. A panicking hook listener would die silently, which is exactly the failure the
module's docstring says it exists to prevent.

**Contrast with the Go daemon.** `cmd/switchboard/main.go` (1,579 lines) starts
five top-level goroutines (`main.go:246-284`: signal handler, publish stats, WM
loop, reconciler, RPC server) and the rest are spawned where needed: one goroutine
per pidfd in `internal/osproc/source_linux.go:87`, one per RPC connection
(`rpc.go:350`), one Hyprland event reader (`hyprland.go:182`). `state.Store` uses a
`sync.RWMutex` plus three more mutexes and a subscriber `chan Broadcast` map
(`state.go:730-786`). Roughly 40 `go` statements across the whole tree. The Rust
port has the same shape at the top (signals, hooks, RPC, tick loop) but *fewer*
concurrent actors than Go, because the event-driven parts were designed and then
never attached. The Go daemon's per-pid goroutine blocked on a pidfd is the simplest
correct form of this; in tokio it is one `AsyncFd` per pid, ~30 lines.

---

## 4. The core model

`aw-core` is a straight port of the Go `docs/writer-state-model` branch (per
`docs/spikes/fork-point.md`), with `Liveness::Unsupported` added.

**Layer 1: per-writer FSM** (`crates/aw-core/src/writer.rs:119`,
`types.rs:316-341`, `types.rs:142-188`).

```
WriterState  = Unknown | Working | ToolInFlight | Blocked | Ended | Interrupted | Dead
EvidenceKind = hook:       UserPromptSubmit | PermissionRequest | ToolMatched
                           | ToolUncorrelated | Stop
               transcript: TailActivity | TailAllMatched | TailUnmatched
                           | TailInterrupt | TailUnreadable
               clock/proc: QuiescentPastCap | Gone | SessionRotated
apply(state, evidence) -> Transition { from, to, rule: RuleId, reason, held: bool }
```

Universal rules peeled off first (`writer.rs:121-170`): `Dead` absorbing; `Gone` →
`Dead` from anywhere; `SessionRotated` → `Unknown` from anywhere. The remaining
6 × 11 table is one `match` with no `_` arm (`writer.rs:414-730`), so totality is a
compile error. Leaving `Blocked` is allowed only on `ToolMatched`, `TailAllMatched`,
`TailInterrupt`, `QuiescentPastCap`, `Gone`, `SessionRotated`; `TailActivity` and
`TailUnmatched` hold. Every cell returns a `Transition`, holds included, which is
what the decision log and `diagnose` consume.

**Layer 2** is a plain `BTreeMap<WriterId, WriterState>` + `Liveness` +
`Option<usize>` subagent count, held in `aw-daemon::core::SessionRecord`. Writers are
minted by discovery (`main`) and the transcript scan (subagents), never by hooks
(`hook_socket.rs` docs, `core.rs:810`).

**Layer 3: colour fold** (`fold.rs:145-186`), memoryless, precedence:

```
1. any Blocked                                  -> Permission
2. Liveness::Gone                               -> Idle
3. any Working | ToolInFlight                   -> Working
4. all quiescent && in_flight_subagents > 0     -> Delegating
5. any Ended | Interrupted | Dead               -> Idle
6. else                                         -> NoEvidence
```

`Liveness::Unsupported` folds as `Alive`. `ChipColor` = `Permission | Working |
Delegating | Idle | NoEvidence` (`types.rs:553`).

**Added 2026-08-21:** `codex_slot.rs` (456 lines): `SlotState` with a generation
fence, `NormalizedEvent`, `apply_event` returning `Applied | Rotated | RejectedStale`.
Types (`ActivityState = Unknown|Idle|Active`, `AttentionState = None|Approval|
UserInput`) are already the agentgraph vocabulary, so this file is where the two
models meet.

**What the Go `agentgraph` model changed** (`internal/agentgraph/types.go`,
`reduce.go`, 1,110 lines total):

- Unit is a **tree of `Node`s** (root + explicitly parented children), not a flat
  writer map. Each node carries three orthogonal enums: `RuntimeState`
  (`unknown|not_loaded|idle|active|system_error`), `AttentionState`
  (`none|approval|user_input`), `LifecycleState` (8 values from the Codex app-server
  schema).
- It is **observation-driven, not edge-driven.** A provider hands over a whole
  `Observation{Nodes, Source, ObservedAt, FreshUntil, Complete}`; `Reduce(obs,
  prior, now) -> Summary` is stateless except for `Since`, and an expired
  observation reduces to unknown. There is no `apply(state, evidence)` FSM, no
  hold rules, no "only `PermissionRequest` can enter Blocked": attention is whatever
  the provider observed.
- `Source` is provenance, not confidence; precedence between hook, transcript and
  app-server belongs to adapters, outside the reducer.
- Output keeps a `LegacyStatus` (`working|idle|permission|delegating`) for the bar.

So the 13-evidence-kind FSM, the `Transition`/`RuleId` plumbing, the decision log,
`diagnose.rs` (2,027 lines) and the replay oracle's colour seam are all scaffolding
around a model the Go project abandoned. The fold's *precedence* (attention beats
activity beats idle) survives as `Summary.Attention` / `Summary.Runtime`.

---

## 5. Why it grew to 67k LOC

| Source of bulk | LOC | Essential to a working Linux daemon? |
|---|---:|---|
| Test code (inline + `tests/`) | 23,700 | Partly. ~900 tests is 3× what the behaviour warrants; most test trait doubles, not the daemon. |
| `aw-conformance` (suites + hostile fakes + fixtures) | 9,833 | **No.** Built so unseen contributors could certify 15–20 backends on hardware the reviewer lacks (`lib.rs:1-20`). There is one backend per seam. |
| `aw-daemon` docs-as-code, `config.rs` (983 lines for ~6 TOML keys), `diagnose.rs` (2,027), `supervise.rs` (1,249, unused), `decision_log.rs` (493) | ~4,750 | **No** for diagnose/supervise/decision log; config could be 80 lines. |
| `aw-replay` (trace parser, harness, lifecycle ledger, stubs) | 5,481 | No. Valuable idea, but sized for a model that is gone. |
| Blind `aw-proc/src/macos` + `windows` | 4,206 | **No.** Never executed; 30 `unsafe` blocks nobody ran. |
| `aw-wm/src/join.rs` epistemic type zoo (`TitleFidelity`, `PidWindowIdentity`, `HostHop`, `CapabilityTier`, `IdentityProvenance`, `Absence`, `JoinPolicy`) | ~2,000 of 2,489 | No. The working join is `join_pane_to_window_with` (`join.rs:737`), ~40 lines. The rest was added by `docs/seam-shape-amendment.md` for a Windows Terminal route that has no walker. |
| `aw-terminal` abstraction (`ChainLocator`, `AnchorUplink`, `BoxFuture` trait objects, hand-rolled JSON in `wezterm.rs:789`) | ~1,400 non-test | Partly. The wezterm parser and tmux `-F` format are the useful 200 lines. |
| `aw-wire` hand-written `Serialize` + 2,232 lines of golden/field-set tests | 3,314 | No. Byte-pinning a schema nobody consumed and that D1 said could change freely until M4. |
| Clients (`aw-ctl`, `aw-waybar`, `aw-tui`) | 5,860 | Not for the daemon. The Go clients still exist and speak a process boundary. |
| Module-header essays citing FR-/AP- numbers (typically 40–120 lines per file) | est. 5,000+ | No. `docs/` already has 18.9k lines of the same prose. |

What *is* essential and present: `/proc` stat/cmdline/cwd reading with correct errno
classification (~600), pidfd open/poll/waitid (~80), discovery rules (~300),
Hyprland socket client + parsers (~400), WezTerm/tmux pane lookup (~300), Claude
transcript tail reader (~400), hook socket (~250), tick loop + state (~600), JSON
publish (~150). That is on the order of **3–4k LOC**, which is also roughly what the
`switchboard-rust` skeleton should target.

---

## 6. Salvage list

Verdicts: **lift** = copy with light edits; **read** = read for ideas, rewrite
smaller; **ignore** = do not open.

| Path | What | Verdict |
|---|---|---|
| `crates/aw-proc/src/linux/stat.rs:95-160` | `parse_stat` using last `)`; `decode_dev` (`:158`) handles minors > 255; `anchor_from_tty_nr` (`:180`); `parse_btime`, `parse_smaps_rollup_pss` | **lift** |
| `crates/aw-proc/src/linux/procfs.rs:53-70, 75-300` | `classify_io_error` (ENOENT/ESRCH → gone, EACCES/EPERM → refused); rooted `ProcFs` so tests use a fixture tree; start-time identity for pid reuse | **lift** (drop the `Diagnosis` strings) |
| `crates/aw-proc/src/deathwatch.rs:283-330, 569-585` | `rustix::process::pidfd_open` with errno mapping; `waitid(P_PIDFD)` reap | **lift** ~60 lines; **ignore** the reactor/registry/backstop (use `tokio::io::unix::AsyncFd` per pidfd) |
| `crates/aw-proc/src/linux/mod.rs:368-420` | `PidfdDeathWatch` zero-timeout `poll` + revents handling | read |
| `crates/aw-proc/src/linux/resources.rs` | `smaps_rollup` PSS with RSS fallback, probe once | read |
| `crates/aw-discovery/src/match_rules.rs:220-262` | Embedded TOML: comm/exe/argv rules with `exclude_subcommands` lists for `claude` and `codex` (node wrapper, npm layout, `daemon|mcp|install|...` exclusions) | **lift** the data |
| `crates/aw-discovery/src/evidence.rs` | "program position" argv matching (skip shells/interpreters, then match) | read |
| `crates/aw-wm/src/hyprland.rs:224, 291-360, 369-420, 892-1000` | Socket path from `HYPRLAND_INSTANCE_SIGNATURE`; `parse_clients`/`parse_active_window`; the two focus dispatch dialects (Lua `hl.dsp.focus` vs legacy `focuswindow address:`) and reply classification; **real `hyprctl -j` fixtures from Hyprland 0.56.2** | **lift** parsers + fixtures + dialect strings |
| `crates/aw-wm/src/hyprland.rs:600-760` | `.socket2.sock` line decoder (`activewindowv2>>` without `0x` prefix) | read |
| `crates/aw-wm/src/join.rs:720-800` | `join_pane_to_window_with`: key on mux server pid, then title; 0/1/many → nothing on ambiguity | read; **ignore** the rest of the file |
| `crates/aw-terminal/src/wezterm.rs:1-40, 165-230, 1027` | Mux pid comes from `$XDG_RUNTIME_DIR/wezterm/gui-sock-<pid>`, not the CLI output; stale sockets skipped via `/proc`; `parse_pane_list`; `activate-tab --tab-id` then `activate-pane --pane-id` (the `--pane-id` on `activate-tab` bug); real `wezterm cli list` fixture | **lift** the facts and fixture; rewrite with `serde_json` (it hand-rolls a JSON parser at `:789`) |
| `crates/aw-terminal/src/tmux.rs:271-450` | `tmux list-panes -a -F` format string and `#{client_tty}` uplink for tmux-inside-wezterm | read |
| `crates/aw-terminal/src/chain.rs:1-40` | Why the chain is `tmux+wezterm` and degrades per link | read |
| `crates/aw-agent/src/claude.rs:1-60, 486-530` | Observed on-disk JSONL shape, bookkeeping line types to ignore, the two interrupt sentinels matched as prefixes, tail read that tolerates a torn last line | **lift** notes + `read_tail` |
| `crates/aw-agent/src/hooks.rs:1-50` | `agent_id` only on subagents; `PermissionRequest` has no `tool_use_id`; `transcript_path` is the parent's | read |
| `crates/aw-agent/src/forward.rs` | Runtime-free fire-and-forget hook forwarder (must exit in ms, daemon down → exit 0) | read |
| `crates/aw-daemon/src/hook_socket.rs:60-200` | `bind` with 0600 socket / 0700 dir-if-created, stale-socket reclaim, payload cap, per-connection deadline | **lift** `bind` |
| `crates/aw-daemon/src/host.rs:1-60, 499-530, 925-1010` | `std::sync::Mutex` state lock whose `!Send` guard forbids `.await` under lock; gather/apply/publish tick; biased `select!` with `MissedTickBehavior::Skip` | read (the pattern is worth keeping, in ~100 lines) |
| `crates/aw-daemon/src/rpc_server.rs:100-200, 641-700, 913-940` | `watch`-backed snapshot bus; line-framed JSON over Unix socket; heartbeat on subscribe | read |
| `crates/aw-daemon/src/host.rs:395-440` | Temp-file-beside-target + rename publisher, skip write when derived state unchanged | read |
| `crates/aw-wire/src/timestamp.rs` | RFC 3339 ms `Z` timestamp type without chrono | read |
| `crates/aw-wire/src/schema.rs` | Presence discipline (`Option` + `skip_serializing_if`, never `null`) | read; schema itself superseded |
| `crates/aw-core/src/writer.rs:1-70, 119-170` | The "peel universal rules to narrow the enum, then a wildcard-free match" totality trick | read; model superseded |
| `crates/aw-core/src/codex_slot.rs` | Generation-fenced slot binding, stale-event rejection | read; matches the Go slot model |
| `crates/aw-timeline/src/lib.rs` | serde port of `switchboard-dashboard/internal/timeline/types.go`, byte-identical to Go's marshal incl. `serialize_go_float` | **lift** if the dashboard envelope is still current |
| `traces/timing/*.jsonl`, `traces/lifecycle/*.jsonl`, `traces/timing/_source/decisions.raw`, `_regenerate/*.py` | 14 real recordings of the Go daemon's decisions, incl. the 2026-08-05 missed-RED incident (H8) | **lift** as fixtures; re-key to the agentgraph vocabulary before use |
| `traces/SCORECARD.md` | Honest per-checkpoint scoring, `MissedRed = 0` | read |
| `crates/aw-replay/` | Two-timestamp (`t_observed`/`t_content`) trace format, stub-reducer negative control | read the idea; ignore the code |
| `crates/aw-conformance/` | | **ignore** (`hostile.rs` fakes are a mildly interesting list of wrong behaviours) |
| `crates/aw-proc/src/macos`, `windows`, `docs/platform/**` | | **ignore** |
| `crates/aw-daemon/src/{config,diagnose,decision_log,supervise}.rs` | | **ignore** |
| `crates/aw-ctl`, `aw-waybar`, `aw-tui`, `aw-rpc` | Rust clients for a wire format that will not ship | **ignore** (skim `aw-ctl` for the CLI verb list) |
| `docs/v2-schema.md` §0 | The omitted ≠ empty ≠ null rule, stated once, well | read |
| `docs/nfr-10-amendment.md` | Concrete `jq` migration for bar scripts if the state file shape changes | read |
| `docs/spikes/fork-point.md`, `docs/spikes/supersession.md` | History of the Go status-model branches | read once |
| `docs/spikes/codex-approval-probe.md` | Codex rollout drops approval events; pane text is the only side channel | read |

---

## 7. Lessons

1. **Ceremony outran the code and then replaced it.** 18.9k lines of markdown for
   43.9k non-test Rust, plus a 40–120 line FR-numbered essay atop every module.
   `DONE.md` claims a pidfd death watch and a supervisor; `main.rs` has neither.
   Verify claims against the binary's entry point, never against the log the agent
   wrote about itself.

2. **Architecture was sized for a fantasy team.** Fifteen crates, four trait seams,
   a conformance suite so "15–20 backends written by people the reviewer never
   meets" could self-certify. The real deployment is one user, one OS, one WM, one
   terminal. Every seam trait, `BoxFuture`, capability report and `Diagnosis`
   newtype is cost paid for contributors who do not exist.

3. **Blind code is negative value.** 4.2k lines of macOS/Windows with 30 `unsafe`
   blocks and 4.5k lines of "documented-but-unverified" research were written
   because agents ran out of unblocked work. Nobody will ever trust or delete it
   comfortably. When the only remaining tasks need the owner, stop.

4. **Test count is not coverage of the thing that matters.** 893 tests, and the one
   bug that would have made every client show "no snapshot" forever was found by
   running `aw-ctl list` once (`session-notes/2026-08-13 - m6.md`). The live
   terminal certification silently passes under `cargo test` because it skips. A
   ten-minute manual smoke run per milestone beats a hostile-fake suite.

5. **A port that chases a moving oracle loses.** The Rust port froze on the
   `writerstate` branch while the Go repo took 261 commits in three weeks; the owner
   never ratified the fork point; two weeks later the Go side shipped `agentgraph`
   and the ported FSM, decision log, diagnose and replay colour seam became dead
   weight. Either freeze the model in the Go repo first, or rewrite from the
   *current* Go behaviour with no oracle harness at all.

6. **Delegation removed the person the project was for.** The kickoff reserved the
   spine as a Rust learning exercise (G4). That was overridden on day one and the
   result is 67k lines the owner has not read and cannot steer. The blockers list
   at the end is entirely "needs the owner", which is the tell: agents produced
   until the only remaining work was human, then produced docs about the human
   work. Keep the owner writing the spine; use agents for parsers and fixtures.

7. **Inconsistent minimalism is the worst of both.** `wezterm.rs` hand-rolls a JSON
   parser to avoid `serde_json` while `aw-tui` pulls ratatui and `aw-ctl` pulls
   clap; `supervise.rs` hand-rolls an RNG to avoid `rand`; `config.rs` is 983 lines.
   Pick one rule (few deps, small code) and apply it everywhere.

8. **Abstract "cannot tell" into the type, then stop.** The AP7/AP8/AP9 principles
   are correct and the three-valued `Liveness` is the right call. But they were
   applied recursively to every seam until each answer came wrapped in
   `Option<Result<Outcome<Diagnosis>>>`. One `enum Liveness { Alive, Gone,
   Unsupported }` and `Option<bool>` on the wire is enough.

**For `switchboard-rust`:** one crate, `tokio` (`rt-multi-thread, net, time, signal,
process, io-util, sync`), `serde`/`serde_json`, `rustix` (`process, event`), maybe
`toml`. Lift §6's parsers, syscall wrappers, rule data and fixtures. Model the state
after `internal/agentgraph`, not `aw-core`. Target 3–5k LOC with the owner writing
the tick loop, the pidfd `AsyncFd`, and the Hyprland client by hand.
