# Switchboard in Rust: an implementation course

Thirteen ordered tasks, about 14 hours of core work plus stretch. When they are
done you have a daemon that finds every Claude Code agent on the machine, tells
you which one wants you, and keeps a history of it. That daemon is the plan in
*Understanding Switchboard's Signals* and `src/model.rs`.

Every task is real product code, not a toy. The tasks are ordered so that each
one drills a gap from your list, on code the next task builds on:

| Gap | Where it is drilled hardest |
|---|---|
| Option / Result combinators, type flow | **T03** (hook adapter), T02 (frames), T05 (transcript), T06 (`/proc`) |
| Ownership and borrowing in real APIs | **T07–T09** (mutating enums in place, split borrows), T12 (moving state through `spawn_blocking`) |
| Deref, `as_ref`, `as_deref`, `&str` / `&Path` | **T01** (newtypes, paths), T06 (`Cow<str>`), T03 (`&'a Value -> &'a str`) |
| Error architecture | **T02** (four error enums, `?` conversions, `transpose`), T04 (line-numbered errors) |
| `Read` / `BufRead` / `Write`, `R: BufRead` | **T04** (tail and history), T12 (`File` + `Tail` across threads) |
| Iterators: `map`/`filter_map`/`flat_map`/`zip`/`fold`/`try_fold` | T03, T05, T06, T09 (`next_deadline`), T08 (`position`, `retain`) |
| async / tokio (engineering-driven) | **T10** (the core actor), **T11** (the socket), **T12** (tasks that own things) |

## How it works

- **You work on `main`.** The scaffold gives you every module with its types,
  doc comments, and signatures. The bodies you write are `todo!("T0N")`.
  Code that is only integration plumbing is provided whole. Read it anyway:
  it uses the things you just built.
- **Tests are the spec.** Each task has a test file under `tests/`:
  `cargo test --test t03_hook`. Most are provided.
- **Three tasks are test-first, and you write the tests** (T02 forwarder, T04
  tail, T07 reducer). The file lists *most* of the test names, each with a doc
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

## The tasks

| # | Task | Core | Stretch | Test file |
|---|---|---|---|---|
| [T01](tasks/T01-names.md) | Identifiers and borrowed strings | 1h | | `t01_names` |
| [T02](tasks/T02-errors.md) | Error architecture | 1h15 | | `t02_errors`, **`t02_forward` (yours)** |
| [T03](tasks/T03-hook-adapter.md) | JSON navigation and the hook adapter | 2h | | `t03_hook` |
| [T04](tasks/T04-tail-and-history.md) | Line-oriented I/O: tail and history | 1h30 | | `t04_history`, **`t04_tail` (yours)** |
| [T05](tasks/T05-transcript.md) | The transcript adapter and usage | 45m | 30m pricing | `t05_transcript` |
| [T06](tasks/T06-proc.md) | Finding agents in `/proc` | 45m | | `t06_proc` |
| [T07](tasks/T07-reducer-turns.md) | Reducer I: routing, sessions, turns, tools | 2h | | **`t07_turns` (yours)** |
| [T08](tasks/T08-reducer-join.md) | Reducer II: gates, the join, subagents | 1h15 | | `t08_join` |
| [T09](tasks/T09-snapshots-and-time.md) | Snapshots and time | 1h | | `t09_snapshot_tick` |
| [T10](tasks/T10-core-actor.md) | async I: the core actor | 1h | | `t10_core` |
| [T11](tasks/T11-socket.md) | async II: the socket | 1h | | `t11_rpc` |
| [T12](tasks/T12-tasks.md) | async III: tasks that own things | 1h | | `t12_tasks` |
| [T13](tasks/T13-capstone.md) | Capstone: run it on your own agents | 30m | open-ended | — |

Order matters up to T09: each task uses the one before it. T10–T12 depend only
on T09 and can be done in any order.

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
are listed as T13 stretch work.
