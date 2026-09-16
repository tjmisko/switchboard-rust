## Go vs Rust (golden) complexity comparison

The Rust version is simpler, and not only because it does less. For the part both versions actually implement, it is about **0.5–0.6x the code**. It has no locks and no generation fences, one function writes the displayed status, and 88% of its tests are pure. It is more verbose in two places, ingestion and support code, and some of the reduction is essential work it has deferred rather than removed. Nothing in either repo was modified.

All LOC below are code lines, with comments and blanks excluded. My counters agree with `tokei` to within 0.1% (Go 68,157 vs 68,244; Rust 8,473 vs 8,470). Rust inline `#[cfg(test)]` modules are counted as test code.

### 1. Lines of code per area

| Area | Go src / test | Rust src / test | Notes |
|---|---|---|---|
| Hook ingestion + forwarder | 160 / 771 | **598** / 571 | Go: ctl hook path 91, rpc 69. Rust is bigger (see verdict). |
| of which: legacy status engine inside `rpc.handleHook` | 196 / 1,170 | — | Status logic, not ingestion. Unreachable in production (`rpc.go:694`, `main.go:282`). |
| Transcript parsing | 1,916 / 2,326 | 422 / 518 | Like-for-like Go (`transcript.go` 654 + `usage.go` 232) is 886. Go only: `usage_tracker.go` 685, `subagents.go` 230, `workflows.go` 115. |
| Status derivation (graph engine, model, Claude coordinator) | 3,430 / 5,175 | 1,835 / 1,461 | Go: `observer.go` 1,150, `agent_observation.go` Claude parts 819, `state` model 560, `agentgraph` 389, `projection.go` 286. Rust: `model.rs` 803, `reduce/` 847, `view.rs` 185. |
| Status, legacy engine (`main.go` self-heal, hydrate) | 302 / 1,100 | — | Go only |
| Subagent fanout / labels / usage wiring | 785 / 1,217 | — | Deferred in Rust: `SnapshotKind::Subagents` has no producer. |
| `statustune` knobs and rule ids | 289 / 296 | — | Rust keeps its constants inline. |
| Discovery / proc / liveness | 627 / 1,042 | 216 / 210 | |
| WM/pane `mapping` (in your discovery row) | 167 / 328 | — | Go only |
| History sink + event types | 935 / 826 | 209 / 167 | |
| Daemon wiring / RPC / publish / persist | 1,283 / 2,754 | 516 / 547 | Go: `state.go` Store 437, `rpc.go` server 395, `main.go` main + reconciler 360, `publishstats` 91. |
| Support (errors, names, paths, time) | 43 / 98 | **502** / 390 | Rust: `error.rs` 171, `names.rs` 153, `time.rs` 103, `paths.rs` 75. Rust test harness adds 311. |
| **In-scope total** | **10,133 / 17,103** | **4,298 / 4,175** | |

Areas that exist only in Go (src / test): Codex 6,289 / 5,512, WM + terminal navigation 2,995 / 2,560, history timeline analysis 3,827 / 3,672, bars/TUI 2,910 / 3,097, pricing 2,301 / 697, federation 1,607 / 2,281, naming 590 / 1,052, other `switchboard-ctl` commands 625 / 594.

**Strict like-for-like** (drop the Go legacy engine, fanout, `statustune`, `mapping` and the extra transcript files): Go 7,364 vs Rust 4,298 (58%). Dropping support code from both gives 7,321 vs 3,796 (52%).

### 2. Structural complexity

| | Go (all) | Go (in-scope) | Rust src |
|---|---|---|---|
| Named types | 418 (331 struct, 30 interface, 57 other) | 138 | 106 (52 struct, 44 enum, 1 alias, 9 `id!` newtypes) |
| Functions + methods | 1,595 | 488 | 202 |
| Mean function length | 18.0 | 18.9 | 16.1 |
| Functions over 60 lines | 71 | 24 | 4 |
| Max nesting depth | 6 | 6 | 4 |
| Functions at depth ≥ 3 | 186 | 66 (13.5%) | 12 (5.9%) |

Nesting depth counts if/for/switch/select/func literals in Go and if/match/loops/closures/let-else in Rust. An else-if chain counts as one level. `tokio::select!` bodies are not parsed, so Rust depth in `rpc.rs` and `core.rs` is slightly understated.

**Longest functions, Go overall** (lines / code lines / depth):
1. `rpc.go:693` `handleHook` 308 / 105 / 4 (two-thirds of it is comments)
2. `codex/observer.go:1020` `applyNotificationLocked` 287 / 281 / 6
3. `history/timeline.go:363` `BuildSwimlanesWithCatalogs` 257 / 198 / 4
4. `cmd/switchboard/main.go:84` `main` 238 / 177 / 4
5. `switchboard-ctl/timeline.go:255` `buildAgentTimeline` 195 / 186 / 5
6. `pricing/calculate.go:15` `EstimateRequest` 191 / 179 / 2
7. `bottombar.go:412` `renderDisplaySnapshots` 172 / 168 / 4
8. `fanout/observer.go:301` `reconcileWorkflowsLocked` 151 / 122 / 4
9. `codex/rollout.go:431` `applyTokenCount` 146 / 139 / 6
10. `fanout/observer.go:147` `Reconcile` 141 / 93 / 4

**Longest functions, Go in-scope:** `handleHook` 308, `main` 238, `reconcileWorkflowsLocked` 151, `Reconcile` 141, `agent_observation.go:486` `applyObservationWithRule` 134, `claude/observer.go:1322` `latchPendingCalls` 131, `observer.go:308` `Observe` 125, `main.go:941` `reconcileOnce` 124 (depth 5), `observer.go:465` `applyHookLocked` 121, `fanout/snapshot.go:131` `snapshotLocked` 120.

**Longest functions, Rust** (lines / code lines / depth):
1. `ingest/claude/hook.rs:47` `hook` 236 / 215 / 3 (one flat match over hook events)
2. `reduce/event.rs:73` `transition` 174 / 154 / 3
3. `reduce/snapshot.rs:19` `apply_snapshot` 120 / 107 / 3
4. `event.rs:368` `ask` 63 / 57 / 2
5. `daemon/rpc.rs:58` `handle` 57 / 50 / 4
6. `time.rs:47` `parse_rfc3339` 56 / 53 / 2
7. `tail.rs:74` `Tail::poll` 56 / 49 / 3
8. `model.rs:1054` `Shown::settle` 54 / 42 / 2
9. `transcript.rs:83` `user_line` 53 / 46 / 3
10. `event.rs:19` `apply_event` 52 / 42 / 2

**Mutable shared state**
- **Go, in-scope:**
  - 15 `Mutex`/`RWMutex`: `state.go:730,748,752,757,768`; `agent_observation.go:77`; `claude/observer.go:138`; `provider.go:60`; `fanout/observer.go:106`; `history/sink.go:32`; `history/agent_state.go:25`; `usage_tracker.go:94`; `discovery.go:318`; `osproc/source_linux.go:29`; `main.go:884`.
  - 3 `Once`, 1 `WaitGroup`.
  - 3 generation counters: `agent_observation.go:78`, `state.go:743`, `state.go:749`.
  - 4 more optimistic fences: `observer.go:349, 362, 380`; `main.go:1020`.
  - Codex adds 3 more locks (`agent_observation.go:88, 94, 100`), its own generation (`codex/observer.go:133, 158`) and 4 `receiveSequence` counters (`codex_hook_transitions.go:65–90`).
  - No mutable globals of note: package-level vars are lookup tables, plus `state.go:817`, which is read once at init.
- **Rust:** 0 locks, 0 atomics, 0 `Cell`, 0 globals, 0 generation counters. The only shared-read location is the `watch<Arc<View>>` channel.

**Timeouts, graces and tunable constants**
- **Go, Claude in-scope:**
  - 10 status timers: `statustune.go:144,145,153`; `observer.go:39, 42, 1469, 1487`; `agent_graph.go:14`; `fanout/observer.go:42, 50`.
  - 8 behaviour toggles in `statustune.Default`.
  - About 14 intervals and caps (`main.go:48, 88–89`, `mapping.go:68/104/147`, tail bytes, sink buffer and retention, `usage_tracker.go:24–27`).
  - 38 named status rule ids (`knobs.go:8` has 18 legacy, `:99` has 20 graph).
  - Codex adds 6 or more (`agent_observation.go:29–31, 132`; `codex_hook_transitions.go:19–20`).
- **Rust:** 16 in total, with no behaviour toggles.
  - 6 status timers: `model.rs:1036–1037`, `reduce/mod.rs:36–45`.
  - 6 daemon settings: `daemon/mod.rs:57–67`.
  - 4 limits: `switchboard-hook.rs:20–22`, `rpc.rs:27–30`, `tail.rs:19`.

**Places that can change a session's displayed status**
- **Go: two engines**, chosen per session by `AgentGraph == nil` (`main.go:1210, 1442`).
  - Legacy engine, 8 direct writes to `Status`: `rpc.go:925`, `main.go:629, 1262, 1456, 1461, 1489, 1510, 1514`.
  - Graph engine, 3 writers: `agent_graph.go:201` (`SetAgentGraph`, reached from `applyObservationWithRule` `agent_observation.go:540`), `agent_graph.go:345` (hydrate), `projection.go:276` (`Restore`).
  - The graph landing path has 8 call sites: Claude at `agent_observation.go:421, 451, 750, 983`; Codex at `codex_hook_transitions.go:264, 525, 739, 1068`.
  - The graph inputs are mutated at about 35 sites in `claude/observer.go`.
- **Rust: one writer**, `Shown::settle` (`model.rs:1054`; initial value from `Shown::new` at `snapshot.rs:159`).
  - It is called only via `reduce::settle` (`reduce/mod.rs:93`), from 5 sites: `event.rs:68`, `snapshot.rs:46, 93, 116`, `tick.rs:21`.
  - Truth is a single function, `Agent::phase` (`model.rs:1181`).
  - To be fair: the inputs it folds over are mutated in about 14 arms of `transition` (`event.rs:73–246`) plus helpers. That is still one reducer and one lag rule.

### 3. Concurrency model

- **Go:**
  - **Store:** an `RWMutex` plus 4 more locks, publish/broadcast generations, and an ordered persistence worker (`state.go:728–775, 832–870`).
  - **Coordinator:** a per-root generation fence; slow I/O runs outside the lock and is rejected if superseded (`agent_observation.go:454–466, 497`).
  - **Observer:** reads outside its lock, then checks identity, ordering and a `slices.Equal` fence (`observer.go:318–380`).
  - **Reconciler:** a detached prepare phase, then apply guarded by a `StartedAt` fence (`main.go:941–1063`).
  - **Goroutines:** one per watched process for death (pidfd) and one per RPC connection.
  - **In-scope counts:** 13 goroutine spawn sites, 10 `make(chan)`, 15 mutexes, 3 `Once`, 1 `WaitGroup`, 3 generations, 4 fences.
  - **Whole codebase:** 29 mutexes, 9 `Once`, 7 `WaitGroup`, 41 `go` sites, 39 `make(chan)`, 66 `select`, 0 atomics.
- **Rust:** the core task is the only owner of the `Board` (`daemon/core.rs:22–49`).
  - 4 channels (`daemon/mod.rs:75–78`): bounded inbox, `watch` view, unbounded effects, bounded history.
  - 6 long-lived tasks (`mod.rs:81–105`), plus one task per connection (`rpc.rs:44`) and one per transcript watcher (`effects.rs:43`).
  - 4 `spawn_blocking` sites, 2 `select!`, 0 locks.
  - Staleness is handled by data, not synchronisation: `ProcKey` start time (`model.rs:123`), the session check (`event.rs:45–49`), `AsOf::supersedes` (`model.rs:491`), and abort-on-replace for watchers (`effects.rs:68–72`).
  - **Caveats where the complexity moved rather than disappeared:**
    - A full inbox silently drops the frame (`rpc.rs:127–135`). The comment says the transcript recovers dropped hooks, but `PermissionRequest` never appears in the transcript, so a dropped one is lost.
    - History entries are also dropped when the appender is full (`effects.rs:47–52`), and the effects channel is unbounded (`mod.rs:77`).

### 4. Test suite shape

| | Go (all) | Go (in-scope) | Rust |
|---|---|---|---|
| Test LOC / src LOC | 36,608 / 31,549 = 1.16 | 17,103 / 10,133 = 1.69 | 4,175 / 4,298 = 0.97 |
| Test functions | 1,190 (+391 `t.Run` sites) | 465 | 241 (202 integration, 39 inline) |
| Pure (no fs write, socket, sleep, goroutine, process) | 653 (55%) | 185 (40%) | 212 (88%) |
| Write the filesystem | 453 | 264 | 15 |
| Real socket | 9 | 8 | 4 (plus 6 in-memory `duplex`) |
| Real sleep | 56 | 6 | 7 |
| Timeout guard | 79 | 48 | 5 |
| Spawn goroutine / task | 74 | 27 | 22 (21 on the tokio runtime, 6 with paused time) |
| Child process | 9 | 3 | 0 |

- **Status tests:** Rust 82 of 82 are pure. Go graph engine 38 of 114, legacy 1 of 9, `handleHook` legacy 2 of 23.
- **Why Go status tests need the filesystem:** Go status code reads transcripts itself (`Observe`, `resolvePending`, `latchPendingCalls`), so its tests have to write files.
- **How tests were classified:** regex over each test body plus the same-package test helpers it calls (and `testsupport.*` / `common::*`). Table-driven cases are not expanded.

### 5. Verdict

**By area**
- **Hook ingestion: Rust is more code (598 vs 160), and it buys something.**
  - Go pushes raw strings through one flat 29-field `rpc.Request` (`rpc.go:48`). Two engines then interpret them later (`statusFromHookEvent` at `rpc.go:1460`, `applyHookLocked` at `observer.go:465`).
  - Rust decides event, cause and confidence at the edge (`hook.rs:47`), which is what makes the reducer pure.
- **Transcript: Rust is simpler (422 vs 886 like-for-like).**
  - Go's `transcript.go` mixes parsing with status decisions (`ResolveKindForCalls`, `BlockedByPendingTool`, `AnchorSince`, lines 575–1250).
  - Rust only maps lines to events.
- **Status: Rust is simpler.** Go has two engines where Rust has one fold and one writer.
- **Discovery, history, wiring: Rust is simpler, but partly by deferral** (pidfd death, retention, graph-event projection, hydration).
- **Support: Rust is more code (502 vs about 43).** Go gets most of this from its stdlib.

**Accidental complexity in Go (removed by the rewrite)**
- The whole legacy engine (498 src + 2,270 test). `handleHook` is unreachable in production, and a shadow comparator runs the two engines side by side (`shadow.go:51`, called at `agent_observation.go:976`).
- Duplicated code: `PendingPrompt`/`PendingWriterMain` (`state.go:348/358` and `projection.go:15/30`), `normalizeAgentID` (`rpc.go:1134`, `observer.go:1215`), `writerQuiescentPastCap` (`main.go:1406`, `observer.go:1581`).
- Go JSON quirks:
  - In-memory and wire twin fields (`StatusSince` `json:"-"` next to `StatusSinceWire`; `Pending` next to `PendingWriters`/`PendingPrompts`).
  - The `agentId` alternate spelling (`switchboard-ctl/main.go:634`).
  - A JSON-encoding change key with clock-clearing rules (`state.go:974`). Rust replaces it with a derived `PartialEq` on `View` (`view.rs:20`, `core.rs:80–89`).
- 38 forensic rule ids, and the whole multi-writer lock and fence apparatus.

**Where Rust is more verbose, and whether it pays**
- **Error enums:** `error.rs` is 171 lines, about 110 of them Display/Error/From boilerplate. It buys 14 tests that assert which field failed.
- **Vocabulary and time:** `names.rs` (153) and a hand-written RFC 3339 parser (`time.rs:47`) exist to avoid pulling in a date crate. That is a dependency choice, not something the language forces.
- **Newtypes** (`model.rs:39–102`) remove the `agent_id` confusion that Go guards with a "normalise exactly once" rule at a choke point.
- **`Effects` plumbing:** about 140 lines (`reduce/mod.rs:115–154`, `effects.rs`). It buys the pure reducer.
- **Borrow-checker shapes:** board splitting (`event.rs:40`, `snapshot.rs:43/74/103`, `tick.rs:15`), `unreachable!` at `event.rs:332, 405`, file handles moved in and out of `spawn_blocking` (`watcher.rs:56–65`).

**Deferred, not removed**
- **Codex:** 6,289 Go src lines. `model.rs` already carries Codex, WM and panel types behind `#![allow(dead_code)]` (`model.rs:29`). `Chip`, `Flags`, `Placement::At`, `Provider::Codex`, `Ownership::Unknown`/`Machine`, `TurnKey::Turn` and `SubagentKind::Teammate` have zero non-test uses outside `model.rs`.
- **Subagent panel and placement producers:** none exist.
- **Usage:** `usage.rs` is never called from the daemon (`usage::` is only referenced in tests).
- **Call identity for PreToolUse hooks with a restricted matcher** (Go `observer.go:1322`, about 330 lines). Rust assumes the matcher is unrestricted (`model.rs:258`).
- **Silent-abort detection:** Go uses the idle-title glyph (`main.go:1483`). Rust's only backstop is `TURN_STALE`, 10 minutes (`reduce/mod.rs:36`).
- **Restart hydration of open approvals:** Rust starts with `Board::default()` and replays the transcript. That cannot restore an open `PermissionRequest`, which the design accepts by showing grey (`model.rs:695–711`).
- **Also not implemented:** pidfd death detection (Rust detects death by sweep diff at about 1 s), history retention, suspension detection, diagnostics.

**Rough decomposition of Go's 10,133 in-scope lines:** about 840 removed accidental complexity, about 2,600 deferred, and about 6,700 core. Against that core, Rust's 4,298 still comes out around 0.6x, even including about 1,000 lines of extra edge typing and support. The remaining saving comes from single-owner concurrency and interpreting hooks at the edge.

### Commands to reproduce

```
tokei ~/Projects/switchboard/cmd ~/Projects/switchboard/internal
cd ~/Projects/switchboard-rust/.worktrees/golden && tokei src tests

S=/tmp/claude-1000/-home-tjmisko-Projects-switchboard-rust/e550f90b-e44b-404a-ad3b-205c7f57c54c/scratchpad
cd $S/goanalyze && go build -buildvcs=false -o goanalyze .
cd ~/Projects/switchboard && fd -e go . cmd internal >| $S/gofiles.txt
$S/goanalyze/goanalyze . $(cat $S/gofiles.txt) >| $S/go.tsv
cd $S/rsanalyze && CARGO_TARGET_DIR=~/.cache/rsanalyze-target cargo build --release --offline
cd ~/Projects/switchboard-rust/.worktrees/golden && ~/.cache/rsanalyze-target/release/rsanalyze . $(fd -e rs . src tests) >| $S/rs.tsv
python3 $S/compare.py $S ~/Projects/switchboard ~/Projects/switchboard-rust/.worktrees/golden loc areastruct struct tests detail

# synchronisation primitives (Go, non-test)
rg -n --glob '!*_test.go' 'sync\.(RW)?Mutex\b|sync\.Once\b|sync\.WaitGroup\b' cmd internal
rg -c --glob '!*_test.go' '(^|[\s;{(])go\s+(func\b|[a-zA-Z_][\w.]*\()' cmd internal
rg -c --glob '!*_test.go' 'make\(chan ' cmd internal

# synchronisation primitives (Rust)
rg -n 'Mutex|RwLock|Atomic|channel\(|tokio::spawn\(|tasks\.spawn\(|spawn_blocking|select!' src

# status writers
rg -n --glob '!*_test.go' '\.Status\s*=[^=]' cmd internal
rg -n 'self\.phase = |settle\(' src
```

Files are in the scratchpad `S`:
- `goanalyze/main.go` (go/ast)
- `rsanalyze/src/main.rs` (syn)
- `compare.py` (area mapping and test tagging)
- `go.tsv`, `rs.tsv` (raw metrics)

The scratchpad is session-scoped; I can move these into the repo or `~/.cache` if you want to keep them. The Rust analyzer's build output is in `~/.cache/rsanalyze-target`.