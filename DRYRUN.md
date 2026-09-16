# Dry run: T05–T09

Starting commit: `59cb82e` (branch `dryrun-b`, T01–T04 pre-filled).

Starting-state note: the pre-filled T01–T04 code passes `t01_names`,
`t02_errors`, `t03_hook` and `t04_history`, but the two learner-written test
files `t02_forward` and `t04_tail` still have `todo!("write this test")`
bodies, so the whole suite never goes green in this worktree. Every task
below reports regressions against the four green files.

### T05 — The transcript adapter and usage
- **Effort:** 7 cycles (1 baseline, 1 after the transcript adapter, 1 after
  usage, 1 clippy, 3 whole-suite runs while sorting out which failures were
  pre-existing). 0 compiler errors: both files compiled on their first
  attempt. Difficulty 2/5: the shapes are the same as T03 with a new table,
  and the tests are precise enough to be the spec. Roughly 20% Rust, 80%
  reading: the fixture, the test file, the sibling `statusline.rs`, and two
  survey documents to learn the field vocabulary.
- **Prerequisites assumed:**
  - `isSidechain` and `isMeta` on user lines mean "not this session
    speaking". Neither appears in the module table or the sheet; the fixture
    line says "a sidechain line must be ignored" and one test case has
    `isMeta: true`.
  - The `rateLimitType` vocabulary (`seven_day_opus`, `seven_day_sonnet`,
    `seven_day_overage_included`, `overage`) needed for `limit_window`. It
    lives in `statusline.rs`'s `WINDOWS` constant and in survey 13, which is
    not on the reading list.
  - What `Origin` to use when `origin.kind` is absent. `Origin::Unknown`'s
    doc comment answers it, but only if you go back to `model.rs`.
  - Rust: `Option::into_iter().flatten()`, `bool::then`, `u128` widening for
    a saturating multiply-then-divide, `BTreeMap::entry().or_default()`,
    writing `Sum` by hand as a `fold`, `str::split_once`, passing a closure
    as `impl Fn` by value. None of these were introduced by T01–T04; T03
    used `filter_map` and `chain` but not `then` or operator traits.
- **Sheet accuracy:**
  - Module table: no row for `isSidechain` or `isMeta`. Both must produce
    nothing or the fixture test fails with an extra `TurnStarted(Human)`.
  - Sheet: "`user_line` turns a `tool_result` block array into events with
    a single `filter` then `filter_map`". True, but the same function also
    owns the interrupt line and the string-content prompt, so it is three
    branches, not one pipeline. The hint reads as if the function were only
    the pipeline.
  - Sheet: "`assistant_line` ... returns `dispatched.chain(ended).collect()`".
    True for the normal path. The API-error path is an early return that also
    needs a `chain` (event then `quota_limits` snapshot, in that order, which
    only the fixture test pins). The shape hint covers half the function.
  - `TurnEnded{Interrupted}`: the model says `turn` is optional; nothing says
    whether the transcript should fill it from `promptId`. I did. No test
    pins it either way.
  - `quota_limits` doc: "Only a rejection with a reset time is worth
    anything." Clear. `used_percent` for a rejection is unspecified; I left
    it `None`.
  - `Rates::cost_micros` doc says "rounded down"; the saturation requirement
    exists only in the test name and assertion. `parse_price`'s rejection
    list (`$.5`, `$1.2.3`, signs) exists only in the test. Both fine as
    tests-are-the-spec, but the sheet could say "the test enumerates the
    rejections; read it first".
  - README reading list points at the whole 562-line survey 10 for T05. Only
    the §5 table of user-line shapes and §6 (error lines) matter.
- **Where I got stuck:** Nowhere for long. Two small frictions: (1) in
  `user_line` I already held `content` after distinguishing string from
  array, but `array_at` wants the parent and a key, so I wrote
  `content.as_array().into_iter().flatten()` by hand; the helper's shape
  does not fit a value you already have. (2) Deciding whether `isMeta`
  should also suppress assistant lines; I applied both skips to every line
  at the top of `line` since nothing in the corpus suggests an assistant
  `isMeta`. No urge to peek at the golden.
- **What this drilled:** `filter` then `filter_map` with `?` inside the
  closure; `chain` of an iterator with an `Option`; `bool::then`; a closure
  parameter typed `impl Fn(Cause, EventKind) -> Input` that captures `ctx`
  and `v` by reference; the lifetime flowing from `&'a Value` to the map
  keys in `usage_by_model`; `Add`/`AddAssign`/`Sum` by hand; folding into a
  `BTreeMap`; integer-only decimal parsing with `split_once`/`strip_prefix`;
  `u128` widening for overflow-safe pricing. Incidental: the transcript
  field vocabulary, and picking which of the eight `ingest::json` helpers
  fits each read.
- **Ordering:** Right where it is. It is T03 again with a different table,
  which is exactly the consolidation a learner needs after T04's I/O. The
  usage half is nearly independent and could be pulled out as a 20-minute
  "operator traits and borrowed keys" mini-task between T04 and T05, which
  would make T05 purely about the adapter. The pricing stretch is well
  scoped and worth keeping optional.
- **Confidence after:** Solid on `filter_map`/`chain`/`then` and on
  reading JSON tolerantly. Shaky on: writing the `usage_by_model` signature
  from scratch (they filled a body under a given signature; kata 1 helps);
  why the `event` closure can be moved into `user_line` and still borrow
  `ctx` (nothing explains that an `impl Fn` argument is passed by value and
  the borrow lives in the closure); and `Sum`'s `fn sum<I: Iterator>` shape.
- **Suggestions:**
  1. Add two rows to the module table: "`isSidechain` or `isMeta` → nothing"
     with the one-line reason.
  2. In `limit_window`'s doc comment list the vocabulary, or point at
     `statusline::WINDOWS` and survey 13 §"Limit names".
  3. Add a test `should_read_an_absent_origin_as_unknown_not_human` so the
     `Origin::Unknown` decision is pinned rather than inferred from
     `model.rs`.
  4. Add a test pinning the `turn` key on the interrupt line, or a sentence
     in the sheet saying to fill it from `promptId`.
  5. Rewrite the shape hint for `assistant_line` as: "an API-error line
     returns early with the failure event chained to `quota_limits`; the
     normal path is `dispatched.chain(ended).collect()`".
  6. Reading list: replace "survey 10" with "survey 10 §5 (the table of user
     line shapes) and §6".
  7. Add a fourth kata: "Why does `user_line(v, event)` compile when `event`
     borrows `ctx` and `v`, and what would break if `line` returned `event`
     instead of calling it?" That is the ownership fact this task quietly
     relies on.

### T06 — Finding agents in `/proc`
- **Effort:** 3 cycles (1 test run, 1 clippy, 1 lib unit tests). 0
  compiler errors. Difficulty 2/5: every function is small, the tests are a
  complete spec, and the two provided I/O functions show exactly how the
  parsers are called. About 40% Rust (the `Cow` return, `AsRef` generic,
  `successors` with a `move` closure, the `FromStr` bound on `number`) and
  60% reading tests to recover the classification rules the sheet said were
  in a doc comment.
- **Prerequisites assumed:**
  - `str::find`/`rfind` returning byte indices you can slice with, and that
    slicing `s[open + 1..close]` is safe because `(` and `)` are one byte.
  - `slice::split(|&b| b == 0)` on a byte slice, and that
    `String::from_utf8_lossy` returns a `Cow` borrowing the input. The
    sheet does name `from_utf8_lossy`, which is enough of a hint.
  - `Option::is_some_and` and `bool::then_some` (used in the provided
    `read_process`, so at least visible).
  - That a closure passed to `iter::successors` must be `move` when it
    captures a `&Path` parameter, or it borrows the local variable holding
    the reference. Kata 1 asks about `'_` but not about `move`, and `move`
    is the error a learner actually hits.
  - The domain: which `claude` invocations are not interactive sessions.
    See sheet accuracy.
- **Sheet accuracy:**
  - Sheet: "`classify` ... The rules are in the doc comment." They are not.
    The scaffold's doc comment says only what `exe: None` means. The rules
    (comm must be `claude`; a readable `exe` must have `claude` in its
    path; `-p`/`--print` anywhere is headless; `daemon` or `mcp` as the
    verb in `argv[1]` is a service) exist only as test cases, and the test
    comment "daemon only matters as the verb" is the sole hint that
    `--model daemon` must pass. I had to invent a `SERVICE_VERBS` list and
    guessed `plugin` as a third entry; nothing says what the full list is.
  - Sheet: "Field N is `fields[N - 3]`." Correct and load-bearing, but the
    field numbers themselves (3 state, 4 ppid, 7 tty_nr, 14 utime, 15
    stime, 22 starttime) appear nowhere in the sheet or scaffold. `Stat`'s
    doc comments give only 22. I knew them from `man proc`; a learner
    without that would need to look them up. The error test also pins the
    field names `"state"`, `"utime"`, `"ppid"`, `"pid"` as the `Missing` /
    `BadNumber` payloads.
  - Sheet: "`session_from_file` ... `procStart` is a **string**." True and
    necessary: the test has a numeric `procStart` that must be rejected.
    Good signpost.
  - Sheet: "`parent_of` and `ancestors` ... one `successors` call and one
    `take`." Accurate. The scaffold body `todo!("T06"); std::iter::empty()`
    with `#[allow(unreachable_code)]` is a little confusing on first read;
    a learner may wonder whether `empty()` is meant to stay.
  - The scaffold's `ancestors` doc "at most `limit` of them" plus the test
    "the starting pid is always yielded" together define `take(limit)` on
    a sequence that begins with `pid`; a limit of 0 yields nothing, which
    no test checks.
  - `key_of` has no mention in the sheet's implement list at all; it is
    only in the test's import list and the scaffold. Trivial once
    `parent_of` exists, but it should be listed.
- **Where I got stuck:** Never blocked. The only thinking time went to the
  `classify` rules and to deciding between `split_once(" (")` and
  `find('(')`/`rfind(')')` for `parse_stat`; the reversed-parens test case
  `6002 )claude( S 1` made me add the `close > open` filter. I wrote
  `stat_of` as a private helper shared by `parent_of` and `key_of` because
  both read and parse the same file.
- **What this drilled:** `ok_or` into `?` across many fields with a
  closure that names the missing field; a generic `number<T: FromStr<Err =
  ParseIntError>>` with `map_err` into a struct variant; `Cow<'_, str>` as
  a return type and why the borrowed case matters; `S: AsRef<str>` with
  `.iter().map(AsRef::as_ref)`; `iter::successors` + `take` + `move`;
  `impl Iterator + '_`. Incidental: the `/proc/<pid>/stat` field numbering
  and the `claude` CLI's subcommand vocabulary.
- **Ordering:** Fine where it is, and a good breather between T05 and the
  reducer. It could even go before T05: nothing in it depends on T05, it
  is shorter, and its `Cow`/`AsRef` content pairs naturally with T01. If
  the goal is "confidence before the reducer", T06 then T05 puts the more
  intricate adapter immediately before T07, which is where its `event`
  closure pattern gets reused.
- **Confidence after:** A learner would feel they understood `Cow` and
  `AsRef` generics, and `?` with `ok_or`. They would be shaky on why
  `move` was needed on the `successors` closure (they will hit E0373 and
  do what the compiler says without understanding that `proc_root` the
  parameter and `*proc_root` the path are different borrows), and on
  whether `+ '_` captures `pid` and `limit` too.
- **Suggestions:**
  1. Put the classification rules in `classify`'s doc comment, as the
     sheet claims, including the verb list, or change the sheet to "the
     rules are the test cases; `SERVICE_VERBS` and `HEADLESS_FLAGS` are
     yours to name".
  2. Add the field-number table to `parse_stat`'s doc comment (`3 state, 4
     ppid, 7 tty_nr, 14 utime, 15 stime, 22 starttime`) or to the sheet.
  3. Add `key_of` to the sheet's implement list.
  4. Add a kata (or extend kata 1): "Remove `move` from the closure. What
     error do you get, and which `proc_root` is being borrowed?"
  5. Replace the `todo!("T06"); std::iter::empty()` body with a comment
     saying "replace this whole body; the `empty()` only keeps the
     signature compiling".
  6. Add a test for `ancestors(_, pid, 0)` so the `take(limit)` semantics
     are pinned at the edge.

### T07 — Reducer I: routing, sessions, turns, tools
- **Effort:** 5 cycles (1 to see the tests fail, 1 after implementing,
  1 clippy, 1 whole suite, 1 deliberate `cargo check` for the kata
  experiment). 0 compiler errors in the real code; 1 induced E0524 for kata
  1. Difficulty 4/5: not because the code was hard to make compile, but
  because most of the two hours went to deciding *what* the reducer should
  do in cases the sheet leaves open, and encoding those decisions in tests
  before there was any code to check them against. Roughly 30% Rust (the
  split borrow in `dispatch`, the re-match in `end_turn`, `let ... else`
  with an assignment in the else arm), 70% reading `model.rs` sections
  4–8, `Shown::settle`, `Agent::phase`, the harness, and working out
  semantics.
- **Prerequisites assumed:**
  - The whole of `model.rs` sections 5–8 (about 750 lines) is now
    needed. The README's table says "T07: `Board`, `Account`, `Agent`,
    `Activity`, `Tool`, `Gate`, `Prompts`, `Unjoined`, `Compaction`", and
    also `Subagents` for T07/T08, `Effect` and `Shown`. That is most of the
    file. It was foreshadowed, but it lands all at once.
  - docs/02 is listed as the reading before T07 and is mostly *not* about
    this code: it argues for a design (`Root`/`Child`, `Writer`,
    `stale_after`) whose names do not match `model.rs` (`Agent`,
    `Subagent`, `Actor`, `TURN_STALE`), and half of it is about Codex,
    which the course has frozen out. What a learner needs from it is §0.1
    (the turn holds a map), §2 (phase is a fold, not a field) and §3 (a
    child raises attention, never lowers it). The rest is noise for T07.
  - `Effects` (provided in `reduce/mod.rs`) collapsing `AppendHistory`
    into an implied publish. Not mentioned anywhere; I only noticed by
    reading the impl, and it changes which tests can assert "no effects".
  - Rust: `let ... else` where the else arm *assigns* the thing that was
    just pattern-matched (used in the `TurnStarted` arm); two-phase
    borrows (kata 3); that matching `&agent.activity` by shared reference
    and then re-matching `&mut` in the same arm is the way to read-then-
    write an enum in place (the `Idle` arm of `end_turn`); that a split
    borrow of two different fields (`agent.activity` via `tools` and
    `agent.prompts`) is legal only because they are separate fields on a
    struct held by `&mut`, not through a method. None of these is taught
    before T07; the katas ask about them after the fact.
  - `Instant::saturating_duration_since` for `held`.
- **Sheet accuracy:**
  - Sheet: "`apply_event`: routing, `Died`, the `/clear` race, adopting a
    session, then `transition` and `settle`." The "`/clear` race" is never
    defined. I read it as: a tracked pid sends an event carrying a session
    the agent does not hold; if the event is `SessionStarted` it is the
    rotation, otherwise it is a late event for the old session and is
    dropped. That reading makes "a pid that's tracked wins over the
    session" interact with "a late event ... is a no-op" in a way the
    sheet does not spell out: an event from a tracked pid naming *another
    agent's* session is dropped rather than routed to that other agent. I
    wrote `should_drop_an_event_whose_tracked_pid_disagrees_with_its_session`
    to pin my reading, and it may disagree with the golden. Either way the
    sheet should say which.
  - Sheet: "Destructure `let Board { agents, accounts } = board;` so you
    can hold `&mut Agent` and `&Account` at the same time." Good and
    necessary hint. But the `Died` arm has to run *before* that
    destructure (it needs `board.agents.remove`), and nothing says so; a
    learner who destructures first and then tries `agents.remove` is fine,
    but one who tries `board.agents.remove` after the destructure gets a
    move error.
  - Sheet: "`TurnEnded` whose key names a different turn is ignored. With
    no key, it ends whatever turn is open." True while a turn is open. The
    rules list says nothing about `TurnEnded` while *idle* (needed for
    "an interrupt landing after `Stop` still records `Stopped`") or while
    *unobserved* (a transcript replay's first line). Both are cases the
    "yours" hints in the test file point at, so the omission is arguably
    intended, but the sheet's rule list reads as complete and is not.
  - Sheet: "`ToolSettled{Failed{interrupt: true}}` ends the turn as
    interrupted." Does not say whether `Denied{by: User}` or a plain
    `Failed` does anything to the turn. I decided neither does. The test
    stub `should_keep_the_turn_when_a_tool_merely_fails` confirms the
    plain case; denial is unpinned.
  - Sheet: "De-escalation and user-caused escalation show immediately."
    The test stub `should_show_stopped_at_once_when_the_user_interrupts`
    says "Working → Stopped is an escalation": true per
    `Severity` (Calm → Notice), and only visible if you read the
    `Phase::severity` impl.
  - Test stub `should_go_idle_on_session_start`: "records exactly one
    history entry". Only true because `board_unobserved` (not
    `board_with`) is used; with `board_with` the agent is already idle and
    there is no entry. The stub does not say which builder to use, and the
    difference between the two builders is the difference between an
    `AgentFound` entry appearing or not.
  - Test stub `should_adopt_the_session_and_watch_its_transcript...`:
    "Check the exact `WatchTranscript` effect, cwd included." The cwd is
    whatever `common::row` fabricates (`/home/u/Projects/p<pid>`), which
    you only learn by reading the harness.
  - Sheet: "`transition` arms: ... `ToolDispatched` / `ToolSettled`
    (**agent actor only**; T08 does subagents)". But `transition`'s doc
    comment says "the first thing this does is hand a subagent's event to
    `subagent_transition`", which is `todo!("T08")`. So a T07 learner
    writes that dispatch line, cannot test it, and every T07 test must
    avoid `from_subagent`. Fine, but say so.
  - Sheet: "`end_turn`, `dispatch` and `settle_call` call T08's
    `*_unjoined` helpers. Until T08, replace those `todo!`s with
    placeholders". Clear and correct; the placeholders make `dispatch`
    read naturally once `blocked()` is in the `map_or`. But `blocked` is
    itself `todo!("T08")` and gets called through the `None` placeholder's
    dead branch, which reads oddly in a "done" T07.
  - Kata 1 in the sheet is about `if let ... = &mut agent.activity` and
    reassigning; the sheet's own hint list says "`mem::take` /
    `mem::replace`" are drilled, but nothing in T07 needed either. I never
    reached for them. Either add a spot that needs one (e.g. taking the
    tool map out of a turn to build `Idle`) or drop them from the gap
    list.
- **Where I got stuck:**
  - The longest pause was semantic, not Rust: whether `apply_event` should
    call `settle` when `transition` returned `false`. I chose "only when
    changed" so that a duplicate really produces no effects, at the cost
    of a due escalation not promoting on a no-op event (that is `tick`'s
    job in T09). The sheet's "last_event is stamped only when something
    changed" suggests the same boundary but does not say it for `settle`.
  - The `Idle` arm of `end_turn`: I wanted to write `*after = Some(..)`
    inside a `match &agent.activity` arm and knew it would not compile,
    so I matched shared, decided, then re-matched `&mut`. A learner will
    hit E0594 here first, and the fix (re-match, or match `&mut` from the
    start and read through the `&mut`) is not obvious.
  - `dispatch`: creating the turn if absent and then binding `tools` from
    the now-guaranteed `Turn` still needs a `let ... else` with an
    unreachable `return false`, because the compiler cannot know the
    assignment made the pattern irrefutable. Small, but a learner will
    wonder if they did it wrong.
  - I did not want to peek at the golden, but I did want the hidden
    test list: the sheet says the golden has 31 tests and mine has 34, and
    I have no way of knowing whether my extra ones encode the same
    semantics. That uncertainty is the point of grading, so it is fine,
    but a learner working alone will feel it.
- **What this drilled:** Returning a `Copy` key instead of `&mut` (with
  the error captured); destructuring `&mut Board` to hold two field
  borrows; `let ... else` on an enum variant with `&mut` bindings and
  assignment in the else; split borrows across `agent.activity` and
  `agent.prompts`; read-then-write of an enum in place; `matches!` for
  cheap variant tests; a `bool`-returning transition table with early
  returns; test-first design against a harness. Incidental: the cost
  model's severity ordering, `Effects`' implied publish, the harness
  builders' fabricated cwd and tty, and deciding order-of-arrival
  semantics for hooks versus transcript lines.
- **Ordering:** This is the jump. T05 and T06 are 2/5 and T07 is 4/5,
  and the jump is mostly "hold the whole model in your head" rather than
  Rust. Two things would flatten it. First, split T07 into T07a
  (`route`, `limits_for`, `settle`, `apply_event` with only
  `SessionStarted`/`SessionEnded`/`Died`/`CwdChanged`, and the routing +
  discovery + death test groups; about 45 minutes, and it exercises the
  `Board` destructure and the `Copy`-key kata) and T07b (turns and tools:
  `idle`, `working`, `end_turn`, `dispatch`, `settle_call` and the
  remaining groups). The T07a tests are the ones with obvious answers and
  give the learner a passing harness before the semantic decisions start.
  Second, move the placeholders for `*_unjoined` into the scaffold
  itself (as `false`/`None` with a `// T08` comment) rather than asking
  the learner to edit T08 stubs; it is busywork that also means a T07
  commit touches T08 code.
- **Confidence after:** A learner would be confident about the
  destructure-then-borrow pattern and about `let ... else`. They would be
  shaky on: why the `route_mut` version fails (the E0524 message talks
  about closures, and a learner may conclude "don't use closures" rather
  than "don't return a conditional `&mut`"); two-phase borrows (kata 3 is
  answerable only if you already know the term); and whether their
  semantic choices (settle-on-no-op, denial and the turn, tracked-pid
  versus foreign-session) are right, because there is no oracle until
  grading.
- **Suggestions:**
  1. Split T07 into T07a (routing, sessions, death, cwd; `route`,
     `limits_for`, `settle`, `apply_event`) and T07b (turns and tools).
     Keep the test file as one but mark the groups with the sub-task.
  2. Define "the `/clear` race" in one sentence in the sheet, and add the
     rule: "an event from a tracked pid carrying a session the agent does
     not hold is dropped unless it is `SessionStarted`".
  3. Add to the rules list: "`TurnEnded` while idle: a non-`Completed`
     outcome replaces `after: None | Completed`; anything else is a
     no-op. `TurnEnded` while unobserved: goes idle with that outcome."
  4. Say whether `settle` runs on a no-op transition. If it must not,
     say why (so that a duplicate is truly silent); if it must, say why
     (so a due escalation promotes without waiting for a tick).
  5. Replace the docs/02 reading with a pointer to its §0.1, §2 and §3
     only, and add a two-line translation table (`Root` → `Agent`,
     `Child` → `Subagent`, `Writer` → `Actor`, `stale_after` →
     `TURN_STALE`). Better: move those three sections' conclusions into
     `reduce/mod.rs`'s module comment, and drop docs/02 from the list.
  6. Ship the `*_unjoined` placeholders in the scaffold so T07 does not
     edit T08 stubs.
  7. Either use `mem::take` somewhere real (suggestion: `end_turn` could
     `mem::replace(&mut agent.activity, idle(..))` and inspect the old
     turn's tools for a future "abandoned calls" history entry) or drop
     `mem::take`/`mem::replace` from the gap list.
  8. In the test stubs, say which builder each test needs
     (`board_unobserved` versus `board_with`) where it matters.
  9. Add a fourth kata: "In `dispatch`, `tools` borrows `agent.activity`
     and `take_unjoined` borrows `agent.prompts` at the same time. Why is
     that allowed, and what happens if you replace the field access with a
     method `agent.prompts_mut()`?"
