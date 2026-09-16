# 14 — Dry run of the course: findings and fix plan

Written 2026-09-16. Three agents each worked a range of the course as a
learner would, with no access to golden, the hidden suites, the answers or
each other, and kept a journal in a fixed format. The journals are verbatim
in `dryrun/`:

| Range | Journal | Starting state | Commits |
|---|---|---|---|
| T01–T04 | [`dryrun/T01-T04.md`](dryrun/T01-T04.md) | clean scaffold | `dryrun-a` |
| T05–T09 | [`dryrun/T05-T09.md`](dryrun/T05-T09.md) | T01–T04 filled from golden | `dryrun-b` |
| T10–T13 | [`dryrun/T10-T13.md`](dryrun/T10-T13.md) | T01–T09 filled from golden | `dryrun-c` |

The task partition falls on file boundaries, so the filled-in states are
exact. One harness artefact to discount: the filled-in copies did not carry
learner-written bodies for the three test-first files (`t02_forward`,
`t04_tail`, `t07_turns`), so the second and third journals note a full suite
that never goes green. A real learner would have written them.

## What the hidden suites said

After the agents finished, their code was run against the suites they could
not see:

| Learner code | Hidden suite | Result |
|---|---|---|
| T01–T04 | `t04_tail_golden` (17) | all pass |
| T01–T04 | `t02_forward_golden` (8) | 2 fail, both on exact message wording (`no reason given`, `daemon refused the frame`) that no sheet states |
| T05–T09 | `t07_turns_golden` (31) | all pass |
| T05–T09 | `t13_capture_findings` (4 end-to-end stories) | all pass |

So the tail semantics and the reducer semantics the learners worried about
all matched golden. The only grading surprise in three ranges was two
strings. That bounds the "underspecification" problem: it is real, it is
cheap to fix, and it is mostly a sheet problem rather than a design problem.

## The ramp, as measured

Difficulty is the agent's own 1–5 score. Cycles are `cargo test` or `cargo
check` runs. Compiler errors are on the real attempt, not the probes.

| Task | Difficulty | Cycles | Compiler errors | Where the time went |
|---|---|---|---|---|
| T01 | 1 | 4 | 0 | 80% reading |
| T02 | 2 | 7 | 0 | 70% deciding wording and semantics |
| T03 | 3 | 4 | 0 | 75% reading and eight untested decisions |
| T04 | 3 | 6 | 0 | half tail semantics, half own arithmetic |
| T05 | 2 | 7 | 0 | 80% reading vocabulary |
| T06 | 2 | 3 | 0 | 60% recovering rules from tests |
| T07 | 4 | 5 | 0 | 70% model comprehension and open semantics |
| T08 | 3 | 5 | 1 (missing import) | 65% tracing tests to helpers |
| T09 | 2 | 3 | 0 | 60% reading tests |
| T10 | 2 | 2 | 0 | 70% reading |
| T11 | 3 | 1 | 0 | 60% reading |
| T12 | 3 | 1 | 0 | 65% reading own earlier code |
| T13 | 1 | n/a | 0 | checking outputs against the machine |

Two caveats the agents raised themselves. The zero error counts are a floor:
the scaffold supplies every signature, so the borrow checker has little
room before T07, and the agents already knew the APIs. Each journal names
the spots where a human learner would hit the compiler (`end_turn`'s
write-through-shared-borrow, `dispatch`'s irrefutable `let else`, `route`
returning `&mut`, `ancestors` needing `move`, the `held` borrow in the
`Limits` arm, a `watch::Ref` across an `.await`, a path moved into a loop
closure). Those spots are where the katas should be.

**Shape.** Flat at 1–3 through T06, one jump to 4 at T07, then down to 3, 2,
and flat at 2–3 through the async range, ending at 1. The single jump is not
a Rust jump: T07 lands the largest slice of `model.rs`, the first test-first
task with an incomplete rule list, and the first sustained borrow-checker
contact all at once. Everything after T07 is easier than T07.

**Verdict against the goal.** The course does build fundamentals before the
hard part, and nothing in it is too hard. Its problem is footing, not
steepness: too much of every task's effort is finding out what was meant.
The learners' three shared complaints, in every range:

1. **Signposts that point at documentation which is not there, or at
   documentation that does not pay.** `docs/03` before T03 (587 lines,
   half Codex, type names the model has replaced, nothing the adapter
   needs). `docs/02` before T07 (argues for a design in a different
   vocabulary: `Root`/`Child`/`Writer` for `Agent`/`Subagent`/`Actor`).
   T06's "the rules are in the doc comment" (they are the test cases).
   T05's module table missing `isSidechain`/`isMeta`. The `model.rs`
   header's T01 row pointing at T02's enums. Each costs ten to thirty
   minutes of "am I missing a file?".
2. **Sheets that can be satisfied for the wrong reason, or not at all.**
   T01's "done when" cannot be met (a `names.rs` unit test needs T02).
   T02's silent-daemon helper hangs up at 400 ms, so a forwarder with no
   read timeout passes the timeout test. T02 tells the learner to build a
   socket harness the skeleton provides. T03 says 30 tests; there are 31.
   `hook.rs` states an actor rule a test contradicts. T13's "watch the
   chip" names a chip that does not exist.
3. **Semantics left open exactly where the learner cannot self-check.**
   Eight adapter decisions in T03, five tail edges in T04, and in T07–T09:
   the `/clear` race, `TurnEnded` while idle or unobserved, `settle` on a
   no-op, `answer` with no call id, `park` duplicates, the stale-closed
   turn's outcome. The hidden suites turned out to agree with the learners
   on nearly all of them, which means the fix is to say so in the sheet or
   pin each with a visible test, and keep the hidden suite for edge cases.

And one complaint specific to the async range: **cancellation safety is
never mentioned**, although T10's loop and T11's subscribe loop are correct
only because `recv` and `changed` are cancel-safe. The best Rust lessons in
T10–T12 are compile errors reported far from their cause, and the sheets
neither warn about them nor turn them into katas.

What worked, and should be done more: the katas, everywhere ("write the type
after each step" in T02, kata 1 in T04 which is literally the line `poll`
needs); the provided test suites in T08 and T09, which carry the join
semantics almost completely; the history half of T04 and the `ask` paragraph
of T08 as models of a fully specified spec; `scanner::run` and `history::run`
as worked examples for T12.

## Fix plan

Three tiers. Tier 1 is correctness: a sheet, comment or test that is wrong,
unmeetable, or points at the wrong place. Tier 2 is restructuring inside a
task that changes no numbering. Tier 3 moves work between tasks and needs a
decision. Tiers 1 and 2 are applied by the fix agents, one per range, from
worktrees off golden; everything lands on golden first and is then
propagated to `exercises` and `main`. Every new or changed test must pass
against golden. Where an item says "per golden", the agent reads golden's
implementation and writes down what it does.

### Range T01–T04

Tier 1:
- T01 sheet "Done when": say that `should_round_trip_every_phase_through_its_name` in `names.rs` stays red until T02, or move it into the T02 test file.
- `cwd_slug`: reconcile byte versus char between sheet, doc comment and unit test name per golden; add a case with a multibyte character (`/home/u/café`) to `t01_names`.
- `model.rs` header, T01 row: remove the wire enums; they belong to T02's row.
- T01 sheet: name `String::into_boxed_str`; point at `gate_of` in `tests/common/mod.rs` as the live reason `Borrow<str>` exists.
- T02: make `one_shot(path, None)` sleep well past any plausible read timeout, so a `send` without `set_read_timeout` fails the timeout test; confirm golden still passes and the test still finishes quickly.
- T02: pin the two hidden-suite strings in the sheet, verbatim: the absent-reason text and the refusal prefix.
- T02: `StartSource` wire vocabulary as a doc comment on the enum in `model.rs`, per golden's `FromStr`.
- T02: state the Display-versus-`source` policy in the `error.rs` module comment, per golden.
- T02 sheet: replace the "you need something to talk to" paragraph with "the helpers `socket` and `one_shot` are provided; read what `None` does".
- T02 sheet, `client.rs` bullet: `Value` implements `Display`; do not reach for `serde_json::to_vec(frame)?`, and say which variant that `?` would land in.
- T03 sheet: "30 tests" → 31. `hook.rs` module comment: state the `SubagentStart`/`SubagentStop` actor exception.
- T03 sheet: add a "decisions the tests do not make" block listing the eight cases the journal names with golden's answer for each; add a fixture and test for each one golden pins.
- T03: name `bool::then` and `Option::filter` next to the no-`match` constraint; add a test for `Stop` with `"background_tasks": []`.
- T04 sheet: state the expected result for each `tail_start` bullet per golden, whether `max_line` counts the terminator, whether whitespace-only lines are blank, and which poll counts a spanning oversized line.
- T04 sheet hazards: `BufRead::lines` continues after `InvalidData`; `seek(SeekFrom::End(0))` returns the length; a multibyte character split across polls must reassemble.
- T04 skeleton: tell the learner to derive expected offsets from the data, or give the skeleton a `len` helper.

Tier 2:
- T03 sheet: a checkpoint line splitting the pure-combinator half (`json.rs`, `hash.rs`, `scrub`) from the adapter half, with the test filter for the first half.
- T04 sheet: `history.rs` first, `tail.rs` second.

### Range T05–T09

Tier 1:
- T05: module table rows for `isSidechain` and `isMeta`; `limit_window` doc lists the vocabulary or points at `statusline::WINDOWS`; a test `should_read_an_absent_origin_as_unknown_not_human`; pin the interrupt line's `turn` key per golden; rewrite the `assistant_line` shape hint to cover the API-error early return.
- T06: put the classification rules in `classify`'s doc comment, verb list included, per golden; field-number table on `parse_stat`; `key_of` in the sheet's implement list; a test for `ancestors(_, pid, 0)` per golden; replace every `todo!(); std::iter::empty()` body pattern (T06 and T09) with a comment saying the `empty()` only keeps the signature compiling.
- T07 sheet: define the `/clear` race in one sentence and state the tracked-pid-versus-foreign-session rule per golden; add the `TurnEnded` while idle and while unobserved rules; say whether `settle` runs on a no-op transition, per golden, and why; say that `transition`'s first line hands subagent events to a T08 function and T07 tests must avoid `from_subagent`; state which builder each test stub needs where it matters; add a kata on the split borrow across `agent.activity` and `agent.prompts`; drop `mem::take`/`mem::replace` from the T07 gap list or use one for real.
- T07 scaffold: ship the `*_unjoined` placeholders (`false`/`None` with a `// T08` comment) so T07 does not edit T08 stubs.
- T08 scaffold: add `SubagentOutcome` to `event.rs`'s imports. Sheet: regroup the arm list by the function each arm lives in; say that T07's `end_turn` and `TurnStarted` must lift parked prompts on the idle path and name the test; pin `answer(call: None, ..)` with a test per golden; add a test that a duplicate `TurnStarted` does not clear a prompt parked between the hook's and the transcript's report; state the `EventsOnly` row lifecycle in the `SubagentEnded` arm comment.
- T09 sheet: `tick` settles with `Cause::Agent, Confidence::Inferred`; a stale-closed turn's outcome per golden; either make `idle`/`working` `pub(super)` or say `tick.rs` builds the literal; label the Discovery and View groups as provided code; "taken before" → "not after"; a kata on why `held` must be dead before the `limits_for` loop.
- `reduce/mod.rs` module comment: the three conclusions of `docs/02` the reducer needs (the turn holds a map; phase is a fold, not a field; a child raises attention and never lowers it) and a vocabulary translation (`Root` → `Agent`, `Child` → `Subagent`, `Writer` → `Actor`, `stale_after` → `TURN_STALE`).

Tier 2:
- T07 sheet: split into T07a (routing, sessions, death, cwd, compaction: `route`, `limits_for`, `settle`, `apply_event` with those arms, and the matching test groups) and T07b (turns and tools). One sheet, one test file, groups labelled.
- Move the `has_unowned_gate` rework and the `phase` reorder from T08 to the end of T07b: sheet text and the `todo!("T08")` marker in `model.rs`.
- T05 sheet: reading pointer becomes survey 10 §5 and §6 only; kata on why `user_line(v, event)` compiles when `event` borrows `ctx` and `v`.

### Range T10–T13

Tier 1:
- T10 sheet: a cancellation-safety paragraph under the `select!` bullet, with a pointer to the tokio docs section, and a kata ("which futures are cancelled when the other arm wins, and why is that safe?"); "compute `next_deadline(&board)` **into a local**"; a kata on `Arc<View>` inside `send_if_modified` (`*current == rendered` does not compile; rustc's `.into()` suggestion compiles and is wrong); "read the header of `tests/t10_core.rs` first"; note that `reduce::Effects` already puts at most one `PublishState` last; mention `TokioInstant::from_std`.
- T11 sheet: `Lines` is not an iterator (`while let Ok(Some(line)) = lines.next_line().await`); a standalone paragraph on never holding `view.borrow()` across an `.await`, and that the error is reported at `tokio::spawn` in `serve`; the statusline adapter takes an account, use `default_account()`; decide the ancestor match (pid or `ProcKey`) per golden and say why, or make it a kata; state the `Subscribe` semantics (later lines ignored, hang-up noticed on the next write) and offer the `select!` as a stretch; reword `resolve`'s doc comment to "the nearest tracked ancestor of `pid`, `pid` included"; comment in `tests/common/mod.rs` that `key()` and `FakeProc::add` agree on `start = 1000` on purpose.
- T12 sheet: open with "`scanner::run` and `history::run` are provided and are the two shapes you need"; the closure needs an owned path (clone per tick or `Arc<Path>`); who parses JSON and that a bad line is skipped; name `Sender::is_closed` and add a test that drops the receiver while the watcher follows a quiet file (must pass against golden); `write_atomic` notes (create the parent, map `temp_sibling`'s `None`, say whether `tokio::fs` or `spawn_blocking`); `sweep_inputs`: collect `seen` before `rows` moves; a kata on transcript file replacement; a comment on the test's `sentinel()`.
- T13 sheet: say what `#![allow(unused)]` is expected to hide and that `model.rs`'s `#![allow(dead_code)]` stays; list the three output paths and that `bind` refuses a live socket; give the `python3` subscribe client inline, printing `name phase reason`, and note `timeout` exits 124; give the settings JSON and the one-line `claude --settings` command verbatim; add the fixture-replay fallback for step 5; replace "watch the chip" with the `phase` field and `lamp`; reorder the stretch list easiest first (`subagentStatusLine` adapter, limit reset expiry, late `TurnStarted`, `explain`, subscriber hang-up, transcript replacement, then inotify and pidfd marked as weekend items with reading pointers); move Codex to a "what comes after" section and drop the reference to open item C1.

Tier 2:
- T11 sheet: `resolve` first ("hardest lesson first, and it needs no socket").
- T12 sheet: implement list reordered `publish.rs`, `scanner.rs`, `effects.rs`, `watcher.rs`.
- README: "T10–T12 in any order" becomes "as numbered", with the one-sentence reason (owner's side of both channels, then the other side, then the tasks that feed them).

### Tier 3: decisions

Each of these was recommended by a journal and each moves work across task
boundaries. None is applied yet.

1. **T06 before T05.** T06 is shorter, depends on nothing after T04, and its `Cow`/`AsRef` content sits next to T01's. T05's `event` closure is then the last thing written before T07 reuses it. Cost: renumbering two sheets, two test files, the `todo!` markers and the header map, or leaving numbers and adding "do T06 first" to the README.
2. **Move `StatError`'s and `RecordError`'s `Display`/`source` out of T02** to T06 and T04, where each has a consumer; keep `RecordError`'s `From` impls in T02, which its test exercises. Cuts T02 by a third and puts each error next to its use. Cost: markers, sheet text, and the T02 test file.
3. **A `usage.rs` mini-task** ("operator traits and borrowed keys", about 20 minutes) between T04 and T05, leaving T05 purely the adapter.
4. **A fourth async task between T12 and T13** that changes code the learner already owns: replace the watcher's ticker with a `select!` against a shutdown signal, or make the subscribe arm notice a peer hanging up. It is the second pass over `select!`, `'static` and `Send` that the async range lacks. Cost: a new sheet, a new test file, and a scaffold that leaves the T12 code in place.

Recommendation: do 1 and 2 before the learner starts T01, since both are
cheap now and expensive after; do 3 only if T05 still feels heavy after 2;
keep 4 as the first post-capstone task rather than adding it to the core.

### Status, 2026-09-16 evening

Tiers 1 and 2 are applied for all three ranges, by three fix agents working
from worktrees off golden, and merged into golden as linear history. Every
scaffold-only edit is recorded in `scaffold-notes/` and was ported to
`exercises` and `main` (one scaffold commit per range on each). Golden's
full suite and clippy are clean; both scaffold trees compile with every
`todo!` stub intact.

One fix-agent change was reverted: an absent transcript `origin` stays
`Origin::Human`, per the F7 decision in `13-capture-fix-handoff.md`; the
comment, sheet and test now state the rule. One golden behaviour change was
kept: a duplicate `TurnStarted` no longer clears a prompt parked between the
hook's and the transcript's report of the same prompt (pinned in the hidden
T07 suite).

Still owed:

- The rendered guide under `~/Guides/` was not updated. Its behaviour rows
  and checkpoint filters change where tests were added or renamed (T02, T03,
  T04, T05, T06, T07, T08, T12) and the T03 and T07 checkpoint commands are
  new.
- Tier 3 is undecided.
