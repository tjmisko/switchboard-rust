# 11 — Handoff: golden audit → redesign planning

Written 2026-09-12 15:53, at the end of the session that built the course and audited the golden.
This is a planning handoff. No fixes have been made yet.

## Goal of the next sessions

Three things:

1. Fix the golden's bugs.
2. Close its test gaps.
3. Update the course: scaffold, task sheets, the guide, and the hidden suites.

The framing, from the user: **this is a redesign, not a port.** Go feature parity is not the target. For each Go behaviour and decision, work out whether it makes sense for the problem ("which agent wants me, and let me get there"), where it can be simplified, and what to drop. Decide from evidence and from the cost model (`docs/04-cost-model.md`), not by copying Go.

## Where everything is

| Thing | Location | State |
|---|---|---|
| Golden reference | branch `golden`, worktree `.worktrees/golden` | 241 tests green, clippy clean. Last commit `95839c7` (`abort_all`). This doc and `docs/audit/` are uncommitted until the handoff commit. |
| Scaffold | branch `exercises`, worktree `.worktrees/exercises` | Golden with `todo!("T0N")` bodies, TDD skeletons, no golden-only files. `f8fd7cc`. |
| Learner workspace | `main` | Scaffold squash-merged: `ee6bd7a`, then `becc14b` (T08/T12 stubs). **Check `git log main` for learner commits before touching `main`.** |
| Course docs | `exercises/README.md`, `exercises/tasks/T01–T13`; kata answers `exercises/ANSWERS-golden.md` (golden only) | |
| Study guide | `~/Guides/Building Switchboard in Rust.html` | Editable sources: `~/.cache/sbr-audit/guide-src/[1-7]*.html`. Concatenate in order to rebuild. |
| Hidden suites (golden only) | `tests/t02_forward_golden.rs`, `t04_tail_golden.rs`, `t07_turns_golden.rs` | |
| Audit reports (full text) | `docs/audit/01-parity.md`, `02-complexity.md`, `03-adversarial.md`, `04-coverage.md` | |
| Failing bug repros | `docs/audit/repro/adv_bugs.rs` (15), `adv_daemon_bugs.rs` (5) | Written against golden's public API; they fail today. Copy into `tests/` to use. |
| Corpus / property / fuzz harnesses | `docs/audit/repro/adv_corpus.rs`, `adv_props.rs`, `adv_history.rs`, `adv_proc.rs`, `adv_tail.rs`, `adv_daemon_fuzz.py`, `adv_cascade.py`, `adv_forwarder.sh`, `adv_shape_*.py` | The `.rs` ones are `#[ignore]`; run with `cargo test --release -- --ignored`. They need `tests/common/`. |
| Mutation driver | `~/.cache/sbr-audit/mutate.py`, results `~/.cache/sbr-audit/mutations.jsonl` | 96 mutations, 43 killed. |
| Go/Rust analysers | `~/.cache/sbr-audit/tools/` (`goanalyze`, `rsanalyze`, `compare.py`) | Commands are in `docs/audit/02-complexity.md`. |

## Workflow invariants — do not break

- **Signatures are a contract across five places.** Hidden suites compile against scaffold signatures. Any public signature change must land in all of:
  - golden
  - the `exercises` branch
  - `main` (as a commit the learner merges, never a rewrite of their work)
  - the task sheet
  - the guide
- **Re-verify every guide checkpoint filter** after renaming tests: `cargo test --test <file> -- --list <filters>`. The script used last time is in the session transcript; rebuild it from the guide's `.cmd` spans.
- **Never merge golden history into `main`.** Deliver scaffold updates as squashed commits.
- **Every learner-owned location carries a `todo!()`.** Where a `todo!` would break earlier tasks, use a stub:
  - `Agent::unowned_gate_anywhere` for T08
  - `Watchers::abort_all`, with a `panicking()` guard in `drop`, for T12
- **Deps are `serde_json` and `tokio` only.** Errors are hand-written. Adding a crate (e.g. `rustix` for pidfd/inotify/connect-timeout) is a user decision.
- **User conventions:** conventional commits, worktrees via `gh worktree`, tests named `should_…_when_…`, no build output or large files under `/tmp`.

## Audit results in one screen

- **Parity** (`01`):
  - About 33% of 111 Go capabilities by count, 37% weighted.
  - Detection of "wants me": about 64%. Delivery (bar, focus, WM, `state.json` compatibility): about 7%.
  - Rust already beats Go on: false green after API errors, stale red after answering, Esc feedback, pid-reuse and zombie handling, waits, elicitation.
- **Complexity** (`02`):
  - Like-for-like, Rust is 0.58× Go's lines (4,298 vs 7,364).
  - Rust has 0 locks and 0 generation fences (Go: 15 and 3), and one status writer against Go's two engines with 11 writers.
  - 88% of Rust tests are pure, against 40% of Go's.
  - About 2,600 Go lines are deferred, not removed: pidfd, directory scan, hydration, Codex.
  - Rust is larger in ingestion (by design) and in support code (errors, names, time).
- **Adversarial** (`03`):
  - 0 panics across 400 MiB of real transcripts, 10k × 300 random reducer steps, socket fuzzing, and forwarder abuse.
  - 20 reproducible bugs (table below).
- **Coverage** (`04`):
  - 53 of 96 mutations survived.
  - 5 spec/doc deviations.
  - About 30 gaps.
  - Several tests can never fail. One t10 test hangs instead of failing.

## Queue A — bugs (from `03` and `04` §A)

**Design?** marks a fix that needs a decision before code. The course column names the tasks, tests and guide rows that change.

| # | Sev | Bug | Likely fix | Design? | Course impact |
|---|---|---|---|---|---|
| 1 | high | Rejected limit never expires. Replayed months-old `quotaLimits` blocks every agent. | Drop past `resetsAt`; stamp with line time; add a reset deadline to `tick` | **yes**: wall clock in `tick`, or convert `resets_at` to an `Instant` on arrival | T05 adapter, T09 tick; guide T05/T09 rows; promotes a T13 stretch item into core |
| 2 | high | Nested `claude -p` hijacks its ancestor's session | Stop the ancestor walk at the first `claude`; rotate only on clear/resume | **yes**: track `-p` runs at all? | T11 `resolve`, T07 `SessionStarted` rules, t07 golden |
| 3 | high | One unreadable `/proc` sweep ⇒ every agent `Died` | `sweep` returns `Result`; death only on `NotFound` | minor | T06 signature, T12 `sweep_inputs` / scanner |
| 4 | high | EMFILE ⇒ accept spin at 100% CPU ⇒ forwarder blocks in `connect()` (no timeout) | Back off on accept errors; bounded connect | **yes**: std `UnixStream` has no connect timeout (thread + deadline, or add a crate) | T11 `serve`, T02 `client::send` + forwarder tests |
| 5 | med-high | Escape before any output missed (24/51 real interrupts lack `interruptedMessageId`) | Also key on the `[Request interrupted by user` prefix | **yes**: contradicts the spec's "key on the field, never marker text" | T05 spec + tests, guide trap text |
| 6 | med | Lost `/clear` hook ⇒ session drift forever | Adopt the sweep's session when `procStart` matches | needs experiment: does `sessions/<pid>.json` update on `/clear`? | T09 provided discovery |
| 7 | med | Subagent gate lifted by any settle of that subagent | Per-call gates for subagents | **yes**: model change to `Subagent.gate` | T08 spec, model, t08 |
| 8 | med-low | Image prompts (`[text, image]` array) start no turn | Array with text and no `tool_result` ⇒ prompt | no | T05 |
| 9 | low-med | Transcript replay floods history with stale transitions | Replay batch: settle once, suppress history | **yes**: replay mode in the model/effects | T12 watcher, T07 settle |
| 10–13 | low | Duplicate settle lifts a parked prompt; duplicate dispatch consumes a prompt; repeated `TurnStarted` clears prompts; duplicate `SessionStarted` / `CompactionStarted` publish | Check for the duplicate before side effects | no | T07/T08 specs + hidden suite |
| 14 | low | Agent discovered under a rejected limit shows Unknown forever | Settle on discovery | no | T09 |
| 15 | low | Unrelated confirmed hook promotes an inferred dwell immediately | Confidence should describe the evidence for the *truth*, not the triggering event | **yes**: cost-model semantics of `settle` | model, T07, docs/04 |
| 16 | low | Compaction expiry needs two ticks | Expire compaction before the stale-turn check | no | T09 |
| 17 | low | `Died` for an untracked pid falls back to session `""` routing | `Died` routes by process only | no | T07 |
| 18 | low | Connections that never finish a line held forever (5000 live) | Read timeout per connection | no | T11 |
| 19 | low | A non-UTF-8 line closes the connection with no reply | Read bytes, reply with an error | no | T11 |
| 20 | nit | `Tail` `max_line` counts the newline; history reader errors on Go `session_start` lines without `cwd` | | no | T04 |
| C-A1–A3 | med | Tooltip `reason()` disagrees with `phase()` for subagent gates, unowned gates, limits without a reset | Derive reason from the same fold | no (golden-only code) | guide T09 view row |
| C-A4 | med | A `Processes` snapshot taken before `Died` resurrects the agent in the reducer; only scanner send order protects it | Tombstones or an `AsOf` check | **yes**: make it a reducer invariant? | T09/T12 |
| C-A5 | low | Late `SessionStarted` for the old session after `/clear` rotates back | Same guard as #2 | with #2 | T07 |

## Queue B — test gaps (from `04`)

- **P0, cost model unpinned:**
  - R1: user cause alone (not paired with Confirmed).
  - R3: inferred Attention must dwell; an unjoined `Unknown` owner renders grey; Elicitation is `Human`.
  - Every precedence pair in `Agent::phase`: Attention > Blocked > Stopped; Delegating > Waiting; Pending delegates.
  - R4: settle without a call id, Abandoned, subagent `PermissionSettled`, `PermissionDenied` keeps its id.
  - Hook `Stop` followed by transcript `end_turn` is a no-op.
- **P1, zero coverage:**
  - compaction
  - unjoined expiry
  - wait deadlines
  - the statusline adapter and Beat
  - Placements / focused
  - panel-owned `SubagentEnded`
  - subagent join
  - hook arms SessionEnd / PostCompact / Elicitation / PermissionDenied (mutation "SessionEnd ⇒ Died" survives)
  - `AsOf` equal-instant boundary
- **P2:**
  - daemon glue: `effects::run`, `history::run` appends, publish coalescing, `bind` stale-socket removal, `offer` on a full inbox
  - the `switchboard-hook` binary
  - every `Reason` text
  - `read_day` hard I/O error
  - `time.rs`: Feb 31 is accepted; offset minutes not range-checked; pre-epoch returns None while format prints 1970
- **Broken tests to fix:**
  - t10 `should_not_wake_subscribers…` can never fail (`has_changed().unwrap_or(false)` after the sender drops). Wrap `view.changed()` in `timeout`; one mutation hung the binary for more than 12 minutes.
  - Three `model.rs` unit tests never call `phase()`.
  - The t05 user-cause test only counts.
  - The t12 "does not exist yet" first half is trivially true.
  - t02_forward_golden and t11 put sockets in `/tmp`.
- **Fixture realism:**
  - `pre_tool_use_ask_subagent.json` uses a prefixed `agent_id` (real ones are bare).
  - Waits element shape is inferred.
  - No statusline fixture.
  - Reducer tests build events with `hash: None` where the adapter sends `Some`.
- **Acceptance bar to set:**
  - Rerun `mutate.py` after fixes. Suggested target: ≥ 85% killed.
  - All `adv_bugs.rs` repros ported and green.
  - `adv_props.rs` invariants run in CI as an `#[ignore]` suite.

## Queue C — redesign decisions: which Go behaviours earn their keep

For each row: what Go does, what Rust does, the question, and how to settle it with evidence. "Leaning" is this session's view, not a decision.

| Behaviour | Go | Rust now | Question | Evidence / experiment | Leaning |
|---|---|---|---|---|---|
| Idle colour | orange "finished" | green (calm) | Is "done since you last looked" information the operator wants? Is it a lamp or a separate marker? | How the attention ring's idle tier is used; the user's own preference | Keep calm green; consider a separate "unseen completion" flag, not a phase |
| Red across daemon restart | hydrates pending prompts from `state.json` | grey, then green after the stale cap | Worth persisting open human gates? | Kill the daemon during a prompt. `PermissionRequest` leaves no transcript line; the `Notification{permission_prompt}` hook at 6 s could re-assert | Persist open human gates only (small) |
| Death detection | pidfd per process | sweep diff every 1 s | Is 1 s death latency ever costly? | Cost model says removal latency is cheap | Keep the sweep; drop pidfd (simplification) — but fix #3 |
| Transcript watch | 5 s tail re-read | 200 ms size poll | CPU cost with N agents vs inotify; Esc latency | Measure the poll's CPU at 10 agents | Keep the poll unless measured cost bites |
| PreToolUse matcher | restricted + 330-line call-id latch | assumes unrestricted | Require an unrestricted PreToolUse (one spawn per tool call) and delete the latch? | Measure hook spawn cost and latency on a busy session | Require unrestricted; document install |
| Subagents | directory scan + stale caps + workflows | hooks (+ panel reducer with no producer) | Minimum signal set? Does `subagentStatusLine` fire with the panel closed (open item A1)? | Run the capture harness (`docs/10-open-items.md` A1) | Hooks + panel; no directory scan |
| Headless `claude -p` | tracked, flagged | rejected by the classifier | Show them? They cause bug #2 | Corpus has 17 nested runs; ask the user | Ignore, and fix attribution |
| Ctrl-Z suspended | flag + history | none | Worth one `stat` field? | Cheap | Keep (tiny) |
| Turn end | `Stop` hook / 5 s tick | a transcript block with `end_turn` | `end_turn` fires up to 32 s early | Corpus timing: `end_turn` vs `stop_hook_summary` / `turn_duration` | End on `Stop` or `turn_duration`, not `end_turn` |
| Transcript path | hook's `transcript_path` | derived from cwd slug | Derived path breaks after `CwdChanged` + `/clear` | — | Carry `transcript_path` from the hook or session file |
| Timers and graces | 10 timers + 8 toggles + 38 rule ids | 6 timers | Validate each constant against the corpus (e.g. `TURN_STALE` vs the longest silent tool call) | `adv_corpus.rs` extension | Fewer constants, each justified by data |
| `state.json` / bar | v3 schema; Go renderers | new schema, no renderers | Emit v3 as a compatibility shim, or rewrite the renderers? | Scope decision | Shim first (reuses Go bar/TUI/navigation), redesign later |
| History | Go schema, feeds `timeline --json` | `shown` records, UTC days, always on | Keep the dashboard working during transition? | docs 06–09 conflict | Decide with the dashboard plan (docs/09) |
| Codex approvals | red after a 30 s grace | grey (R3) | Is grey acceptable for unowned gates, or does the operator lose real prompts? | Needs Codex capture | Unresolved; Codex is deferred |
| Navigation / WM | Hyprland + wezterm + ring/cycle/pick | none | In scope for the course? | User decision | Probably a later phase, not the current 13 tasks |
| Naming | projectname abbreviations, `sessions/<pid>.json` name, labels | title, else cwd basename | Which naming does the operator actually read? | User preference | Use the session file `name`; drop abbreviations |

Evidence tools already built:
- `adv_corpus.rs`: replays every real transcript through adapter and reducer.
- `adv_shape_*.py`: structure-only corpus statistics.
- `adv_props.rs`: random reducer invariants.
- `mutate.py`: mutation testing.
- The hook-capture harness is **still unrun**. It is the highest-value experiment for rows marked "A1" and for bugs #6 and #15.

## Queue D — course updates once A–C are settled

- **Per fixed bug:** update the golden and its tests, the scaffold stub/doc comment, the task sheet spec, the guide's behaviour row and checkpoint filter, and the hidden suite where the task is TDD (T02/T04/T07).
- **Model changes** (per-call subagent gates, replay mode, confidence semantics) ripple into T07–T09 specs and the `model.rs` changes listed in `exercises/README.md`.
- **New signatures** (`sweep -> Result`, `client::send` connect timeout) change T06 / T12 / T02.
- **Estimates:** if limits expiry (#1) and the interrupt fallback (#5) become core behaviour, T05 and T09 grow by about ½ h each. Re-balance the 10–15 h budget.
- **If `main` has learner work by then:** ship changes as a commit that touches only files the learner hasn't started, or list the manual merges needed.

## Recommended next session

1. Orient on this doc and `docs/audit/03-adversarial.md`.
2. Walk the user through the Queue C rows marked "yes/ask" and the Design? rows in Queue A: #1, #2, #4, #5, #7, #9, #15, C-A4. Get decisions.
3. Run the hook-capture experiment (docs/10 A1) if subagent, Esc or `/clear` behaviour is still unknown.
4. Then fix in this order: high bugs → P0 tests → low bugs → P1 tests → course propagation. Run the mutation driver at the end.

## Open questions for the user

- Wall clock inside `tick`, or deadlines converted at ingest?
- Allow one extra crate (`rustix`) for a connect timeout, pidfd and inotify, or stay std-only?
- Track `claude -p` runs at all?
- Should the course grow a navigation / `state.json`-shim phase, or stay detection-only?
- Is marker-text interrupt detection acceptable as a fallback, given the evidence?
- Persist open human gates across restarts?
