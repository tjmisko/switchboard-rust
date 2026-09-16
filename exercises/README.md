# Switchboard in Rust: an implementation course

Fifteen ordered tasks, about 16 hours of core work plus stretch. When they are
done you have a daemon that finds every Claude Code agent on the machine, tells
you which one wants you, and keeps a history of it. That daemon is the plan in
*Understanding Switchboard's Signals* and `src/model.rs`.

Every task is real product code, not a toy. The tasks are ordered so that each
one drills a gap from your list, on code the next task builds on:

| Gap | Where it is drilled hardest |
|---|---|
| Option / Result combinators, type flow | **T03** (hook adapter), T02 (frames), T07 (transcript), T05 (`/proc`) |
| Ownership and borrowing in real APIs | **T08–T10** (mutating enums in place, split borrows), T13 (moving state through `spawn_blocking`), T14 (the same, in code you already own) |
| Operator traits and borrowed map keys | **T06** (`Add`, `Sum`, a `BTreeMap<&'a str, _>` that borrows its input) |
| Deref, `as_ref`, `as_deref`, `&str` / `&Path` | **T01** (newtypes, paths), T05 (`Cow<str>`), T03 (`&'a Value -> &'a str`) |
| Error architecture | **T02** (four error enums, `?` conversions, `transpose`), T04 (line-numbered errors) |
| `Read` / `BufRead` / `Write`, `R: BufRead` | **T04** (tail and history), T13 (`File` + `Tail` across threads) |
| Iterators: `map`/`filter_map`/`flat_map`/`zip`/`fold`/`try_fold` | T03, T07, T05, T10 (`next_deadline`), T09 (`position`, `retain`) |
| async / tokio (engineering-driven) | **T11** (the core actor), **T12** (the socket), **T13** (tasks that own things), **T14** (`select!`, cancellation and shutdown, second pass) |

## How it works

- **You work on `main`.** The scaffold gives you every module with its types,
  doc comments, and signatures. The bodies you write are `todo!("T0N")`.
  Code that is only integration plumbing is provided whole. Read it anyway:
  it uses the things you just built.
- **Tests are the spec.** Each task has a test file under `tests/`:
  `cargo test --test t03_hook`. Most are provided.
- **Three tasks are test-first, and you write the tests** (T02 forwarder, T04
  tail, T08 reducer). The file lists *most* of the test names, each with a doc
  comment saying what it must pin down, and asks you to find a few more
  yourself. You write the bodies first, watch them fail, then implement. When
  you're done I run a hidden golden suite for the same behaviours against your
  code. It exists to find the edge cases your tests missed, not to replace
  them.
- **Katas.** Most tasks end with two or three short questions of the form "why
  does this not compile" or "write the type after each combinator". Answer
  them in `exercises/answers/T0N.md` before looking anything up.
- **Grading.** Commit, then tell me "grade T0N". I run the hidden tests and
  clippy against your commit, read your diff against the golden, and review
  both correctness and idiom. Diverging from the golden is fine when it's a
  choice; I'll say whether I think it's a good one.

Conventions carried over from the rest of your setup: tests are named
`should_<behaviour>_when_<condition>`, commits are conventional, early returns
beat nesting.

## What to read, and what to ignore

The repository carries far more documentation than the course needs. This
is the whole reading list, in order:

1. This file.
2. The header of `src/model.rs`, which maps each of its sections to the
   task that first needs it. Read a section when its task arrives, not before.
3. Before T03: the field table in
   `docs/product/survey/14-hook-events-verified.md` §1.1 and the module
   comment at the top of `src/ingest/claude/hook.rs`. That is the whole
   hook-to-event mapping. Nothing else.
4. Before T07: `docs/product/survey/10-transcript-facts.md` §5 (the table of
   user-line shapes) and §6 (error lines). Skip the rest.
5. Before T08: the module comment at the top of `src/reduce/mod.rs`, which
   carries the three conclusions the reducer rests on and translates the
   design documents' vocabulary into the code's. Then, only if you want the
   argument behind them, `docs/02-session-machine.md` §0.1, §2 and §3 and
   `docs/03-types-and-transitions.md` §4 and §13. Both documents use older
   type names and spend half their length on a second provider the course
   has frozen out; read them for the reasoning, not the names.
6. Before T10: [`docs/04-cost-model.md`](../docs/04-cost-model.md), which is
   short and is where every timeout and hysteresis constant comes from.
7. The [architecture explorer](../docs/architecture-explorer/README.md),
   whenever you want the picture.

Ignore everything else under `docs/product/` and `todo.md`. Those are the
owner's notes for the product track: the rewrite plan, the surveys, the
audits, the handoffs. They change without notice and the course does not
depend on them. The course is frozen as Claude-only and hooks-only until T15
is graded; a Codex adapter exists on the `codex` branch and is the material
for what comes after.

## The tasks

| # | Task | Core | Stretch | Test file |
|---|---|---|---|---|
| [T01](tasks/T01-names.md) | Identifiers and borrowed strings | 1h | | `t01_names` |
| [T02](tasks/T02-errors.md) | Error architecture | 1h | | `t02_errors`, **`t02_forward` (yours)** |
| [T03](tasks/T03-hook-adapter.md) | JSON navigation and the hook adapter | 2h | | `t03_hook` |
| [T04](tasks/T04-tail-and-history.md) | Line-oriented I/O: history, then tail | 1h45 | | `t04_history`, **`t04_tail` (yours)** |
| [T05](tasks/T05-proc.md) | Finding agents in `/proc` | 55m | | `t05_proc` |
| [T06](tasks/T06-usage.md) | Usage: operator traits and borrowed keys | 20m | 30m pricing | `t06_usage` |
| [T07](tasks/T07-transcript.md) | The transcript adapter | 40m | | `t07_transcript` |
| [T08](tasks/T08-reducer-turns.md) | Reducer I: routing, sessions, turns, tools (T08a, T08b) | 2h | | **`t08_turns` (yours)** |
| [T09](tasks/T09-reducer-join.md) | Reducer II: gates, the join, subagents | 1h15 | | `t09_join` |
| [T10](tasks/T10-snapshots-and-time.md) | Snapshots and time | 1h | | `t10_snapshot_tick` |
| [T11](tasks/T11-core-actor.md) | async I: the core actor | 1h | | `t11_core` |
| [T12](tasks/T12-socket.md) | async II: the socket | 1h | | `t12_rpc` |
| [T13](tasks/T13-tasks.md) | async III: tasks that own things | 1h | | `t13_tasks` |
| [T14](tasks/T14-second-pass.md) | async IV: the second pass, in code you own | 1h | 1h file replacement | `t14_second_pass` |
| [T15](tasks/T15-capstone.md) | Capstone: run it on your own agents | 30m | open-ended | — |

Order matters throughout. Up to T10 each task uses the one before it. T11–T13
each compile and test on their own against T10, but do them as numbered: T11
builds the two channels from the core's side, T12 uses both from the other
side, and T13 spawns the tasks that feed them. T14 changes code T12 and T13
wrote, so it comes after both. T08 is the course's one real jump and its
sheet is in two halves, T08a and T08b, with a passing suite between them.

Numbering changed on 2026-09-16, after a dry run of the course: `/proc` moved
ahead of the transcript adapter, usage became its own short task, a fourth
async task was added, and the capstone became T15. Documents under
`docs/product/` written before that day use the old numbers: old T05 is now
T07, old T06 is T05, old T07–T12 are T08–T13, and old T13 is T15.

## Rules of the road

- **Dependencies:** `serde_json` and `tokio` only. Errors are written by hand.
  There is no `thiserror` or `anyhow`, because writing them yourself is the
  exercise.
- **Do not change public signatures** without saying so. The hidden tests
  compile against them. If you think one is wrong, that's a good conversation
  to have, and several of them were contested.
- `cargo clippy --all-targets` should be clean when you grade.
- `lib.rs` starts with `#![allow(unused)]` so the stubs don't bury you in
  warnings. Delete that line at the end and fix whatever it was hiding.

## What changed in `model.rs`, and why

The golden made these changes to the model you designed. Every one of them was
forced by a test that could not be written otherwise. Push back on any of them.

1. **`Event` gained `process: Option<ProcKey>` and `confidence: Confidence`.**
   Routing by session alone cannot find the agent for a `/clear`, because it
   arrives carrying a session id the board has never seen. Hook frames carry
   the forwarder's pid, which resolves to a tracked ancestor. And `settle`
   needs a confidence the event did not carry: a hook is `Confirmed`, a
   replayed transcript line is `Inferred`.
2. **`Agent::session` is `Option<SessionId>`.** An agent exists from the first
   sweep, and a sweep can only name the session when
   `~/.claude/sessions/<pid>.json` exists and its `procStart` matches field 22
   of `/proc/<pid>/stat`. (It does match. I checked against a live process.)
3. **`TurnStarted.turn` and `TurnEnded.turn` are `Option<TurnKey>`.**
   `prompt_id` is optional on the hook base, and assistant transcript lines do
   not carry it at all.
4. **`PermissionAsked` and `Unjoined` gained `kind: GateKind`.** Otherwise a
   question tool's gate loses its kind the moment it is parked.
5. **`SnapshotKind::Waits { session, rows: Vec<WaitRow> }`.** `Stop` yields an
   event plus this snapshot, and it is the only source for `Phase::Waiting`.
6. **`has_unowned_gate` now sees tool and subagent gates, and runs before
   `Turn → Working`.** The sketch only checked unjoined prompts. That meant a
   Codex gate that joined its call rendered green mid-turn, which violates R3.
7. **`Effect` was reshaped:** `AppendHistory(HistoryEntry)`,
   `WatchTranscript { process, session, cwd }`, and `Forget`. `SignalWaybar`
   and `BindPidfd` were dropped because they were platform-specific.
   `ReconstructFromTranscript` was folded into `WatchTranscript`, whose watcher
   replays the tail first.
8. **The reducer moved out** of `model.rs` into `src/reduce/`.
9. **`InputHash` is FNV-1a, not sha256.** Both sides of the join are hashed by
   this crate, so the hash only has to agree with itself, and there's no
   crypto crate.
10. Small ones: `ProcessRow` gained `session` and derives `PartialEq`, `Gate`
    derives `PartialEq`, and `Reason::Interrupted` was added so a Stopped chip
    has a tooltip.
11. **From the September 2026 signal capture** (`tools/hookcap/`):
    `WaitKind` gained `Workflow` and `Other` (the captured task types are
    `shell`, `monitor`, `workflow`); `Origin` gained `Unknown`, because
    `UserPromptSubmit` never carries `source`; `ProcessRow` gained `name`,
    read from `~/.claude/sessions/<pid>.json`, because `session_title` is
    never sent; and `reduce::SUBAGENT_STALE` bounds a subagent row that no
    hook or panel mentions, because a third of children never report a stop.

Two things the golden does *not* do that the plan says it should, because they
need a syscall crate: **pidfd** (the scanner diffs sweeps instead, and a
vanished `ProcKey` becomes `Died`), and **inotify** (watchers poll the file
length every 200 ms, which is inside the cost model's latency budget). Both
are listed as T15 stretch work.
