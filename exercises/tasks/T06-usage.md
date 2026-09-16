# T06 — Usage: operator traits and borrowed keys

**Core: 20m · Stretch: 30m** · Gaps: `Add` / `AddAssign` / `Sum` by hand, a
map whose keys borrow its input, `BTreeMap::entry().or_default()`, `u128`
widening, integer decimal parsing · Test: `cargo test --test t06_usage`

One API response is written to the transcript as several `assistant` lines,
one per content block, and every one of them repeats the response's full
`usage`. Summing per line counts a response once per block. So usage is
keyed by response first (`message.id`, else `requestId`) and only then
summed. That is the whole task: a small arithmetic type, and a fold that
does not count twice. T07 reads the same lines for their events and never
touches usage.

## Implement

**`src/usage.rs`** (core):

- `Usage::from_json(u: &Value) -> Option<Usage>` and `Usage::total`. Absent
  counters are zero; a `usage` that is not an object is no usage at all.
- `Add`, `AddAssign` and `Sum` for `Usage`. `AddAssign` is one line once
  `Add` exists (`*self = *self + rhs`; `Usage` is `Copy`). `Sum` is a `fold`
  starting from `Default`, and its signature is the trait's:
  `fn sum<I: Iterator<Item = Usage>>(iter: I) -> Usage`. Write the fold with
  `Add::add` as the function, not a closure.
- `response(line: &Value) -> Option<Response<'_>>`: the usage an `assistant`
  line reports, keyed by the response it belongs to. `None` for any other
  line, for a line with no key, and for the synthetic error lines whose
  `model` is `<synthetic>`. Every field of `Response` borrows from `line`.
- `usage_by_model<'a, I: IntoIterator<Item = &'a Value>>(lines: I) -> BTreeMap<&'a str, Usage>`.
  Two maps: first key → `Response`, collected straight from
  `filter_map(response)`, so the last line of a response wins; then
  `entry(model).or_default() += usage` over the first map's values.

**`src/usage.rs`** (stretch): `Rates::cost_micros`, `parse_price`,
`rates_for` and `transcript_cost`. Money is integer micro-dollars and no
floats, anywhere. Multiply in `u128` and divide before narrowing, so
`u64::MAX` tokens saturate rather than overflow. `parse_price` is
`split_once('.')` on the digits after the `$`, and the test enumerates every
rejected spelling (`$.5`, `$1.2.3`, signs, seven decimal places): read
`should_parse_prices_without_floats` and
`should_price_usage_in_micro_dollars_without_overflow` before writing
either. `rates_for` tries the exact id, then the id with a bracketed
context suffix such as `[1m]` removed. `transcript_cost` must *report* the
models it couldn't price rather than counting them as zero cost; `partition`
is the shape. The Go daemon has a live bug of exactly that kind, with
`inference_geo: "not_available"`.

## The ownership lesson

The map `usage_by_model` returns has `&'a str` keys, and `'a` is the
lifetime of the `Value`s it was given. Nothing is copied: a key is a slice
of a `model` string inside one of the caller's parsed lines, and the map is
usable exactly as long as those lines are. That is why the signature takes
`&'a Value`s and not lines it would parse itself. A version that took
`&[String]` would parse into `Value`s that die when the function returns,
and the borrow checker would refuse to hand out keys into them
(E0515). Either the caller parses and the map borrows, or the function
parses and the map must own `String` keys. The signature says which.

`Response<'a>` is the same lesson in miniature: `Copy`, three borrowed
fields, a view of one line. Collecting `(r.key, r)` pairs into a `BTreeMap`
gives you a map whose keys and values both borrow the same lines, which is
fine, and which the compiler checks without any annotation beyond the one
`'a` on the function.

## Katas

1. `usage_by_model<'a, I: IntoIterator<Item = &'a Value>>(lines: I) -> BTreeMap<&'a str, Usage>`.
   Why can't this take `lines: &[String]` and parse them inside, while
   keeping the same return type?
2. Give the type after each step:
   `lines.into_iter()` → `.filter_map(response)` → `.map(|r| (r.key, r))`
   → `.collect::<BTreeMap<_, _>>()`.
3. `let total: Usage = usages.iter().sum();` does not compile, and
   `usages.into_iter().sum()` does. Why, and what impl would make the first
   form work?

## Done when

The three core tests in `t06_usage` pass. The four pricing tests are the
stretch.
