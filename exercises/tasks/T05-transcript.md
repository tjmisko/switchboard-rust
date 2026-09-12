# T05 — The transcript adapter and usage

**Core: 45m · Stretch: 30m** · Gaps: `flat_map`, `filter_map`, `chain`,
`bool::then`, collecting borrowed keys, operator traits · Test:
`cargo test --test t05_transcript`

The transcript carries the user-caused edges that no hook reports, Escape
during generation above all. It also repeats a response's usage once per
content block. That second fact is a classic iterator bug to get wrong.

## Implement

**`src/ingest/claude/transcript.rs`**: `line`, `user_line`, `assistant_line`,
`quota_limits` and `limit_window`. The table in the module comment is the
spec. `should_turn_the_fixture_into_exactly_this_sequence` pins the whole
fixture, so run it early and often.

Shape to aim for:

- `assistant_line` builds `dispatched` (a lazy iterator of `ToolDispatched`)
  and `ended` (an `Option`), then returns `dispatched.chain(ended).collect()`.
- `user_line` turns a `tool_result` block array into events with a single
  `filter` then `filter_map`, and the `?` operator *inside* the closure.

**`src/usage.rs`** (core): `Usage::from_json`, `Add`, `AddAssign`, `Sum`,
`response`, `usage_by_model`.

**`src/usage.rs`** (stretch): `Rates::cost_micros`, `parse_price`,
`rates_for` and `transcript_cost`. Use integer micro-dollars and no floats.
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

## Done when

The core tests in `t05_transcript` pass. The pricing tests are the stretch.
