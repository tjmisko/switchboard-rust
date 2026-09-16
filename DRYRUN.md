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
