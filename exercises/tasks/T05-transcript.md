# T05 — The transcript adapter and usage

**Core: 45m · Stretch: 30m** · Gaps: `flat_map`, `filter_map`, `chain`,
`bool::then`, collecting borrowed keys, operator traits · Test:
`cargo test --test t05_transcript`

The transcript carries the user-caused edges that no hook reports, Escape
during generation above all. It also repeats a response's usage once per
content block. That second fact is a classic iterator bug to get wrong.

## Read first

Survey 10, §5 only (the table of `user` line shapes) and §6 (error and limit
lines), in `docs/product/survey/10-transcript-facts.md`. Nothing else in that
file is needed here. The fixture `tests/fixtures/claude/transcript/session.jsonl`
is twenty-one lines; read it next to the module table before you write a line.

## Implement

**`src/ingest/claude/transcript.rs`**: `line`, `user_line`, `assistant_line`,
`quota_limits` and `limit_window`. The table in the module comment is the
spec, the `isSidechain` and `isMeta` rows included: both mean "not this
session speaking" and produce nothing, whatever the line's type. The fixture
has one of each, and `should_turn_the_fixture_into_exactly_this_sequence`
pins the whole fixture, so run it early and often.

Shape to aim for:

- `line` drops sidechain and meta lines before it looks at `type`, builds the
  `event` closure once (stamping process, session, confidence and the parsed
  timestamp), and hands it to one of the two line functions.
- `user_line` is three branches, not one pipeline. The interrupt line
  (`interruptedMessageId`) ends the turn as `Interrupted`, keyed by `promptId`
  exactly as a prompt is; with no `promptId` the key is `None` and it ends
  whatever turn is open. The string-content line is a prompt, unless it opens
  with one of the local-command prefixes; a prompt with no `origin` at all is
  `Origin::Unknown` and agent-caused, because "the source did not say" is not
  evidence of a person. Only the block-array branch is the pipeline: a single
  `filter` then `filter_map`, with the `?` operator *inside* the closure.
- `assistant_line`: an API-error line returns early with the failure event
  chained to `quota_limits`, event first and snapshot second; the fixture test
  pins that order. The normal path builds `dispatched` (a lazy iterator of
  `ToolDispatched`) and `ended` (an `Option`) and returns
  `dispatched.chain(ended).collect()`.
- `quota_limits`: a rejection with a reset time is the only thing worth a
  snapshot. `used_percent` stays `None`; the transcript never reports one.
- `limit_window`: the vocabulary is in its doc comment, and
  `statusline::WINDOWS` lists the per-window keys. The per-model seven-day
  names fold into `SevenDay`; `seven_day_overage_included` is an overage.

**`src/usage.rs`** (core): `Usage::from_json`, `Add`, `AddAssign`, `Sum`,
`response`, `usage_by_model`. `Sum` is a `fold` starting from `Default`.

**`src/usage.rs`** (stretch): `Rates::cost_micros`, `parse_price`,
`rates_for` and `transcript_cost`. Use integer micro-dollars and no floats.
The tests enumerate the rejected price spellings and the saturating multiply;
read `should_parse_prices_without_floats` and
`should_price_usage_in_micro_dollars_without_overflow` before writing either.
`transcript_cost` must *report* the models it couldn't price rather than
counting them as zero cost. The Go daemon has a live bug of exactly that shape,
with `inference_geo: "not_available"`.

## Katas

1. `usage_by_model<'a, I: IntoIterator<Item = &'a Value>>(lines: I) -> BTreeMap<&'a str, Usage>`.
   Why can't this take `lines: &[String]` and parse them inside, while keeping
   the same return type?
2. In `lines().iter().flat_map(|v| line(v, &c))`, what is the item type of the
   `flat_map`, and why does `Vec<Input>` qualify as the closure's return?
3. `(cond).then(|| event(..))` versus `cond.then_some(event(..))`. Which one
   evaluates `event` when `cond` is false?
4. `user_line(v, event)` takes `event` by value, and `event` borrows `ctx` and
   `v`. Why does that compile, and what would break if `line` returned `event`
   to its caller instead of calling it?

## Done when

The core tests in `t05_transcript` pass. The pricing tests are the stretch.
