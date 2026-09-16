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
