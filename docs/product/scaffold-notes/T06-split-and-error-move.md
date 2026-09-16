# Scaffold notes for `fix-d` (T06 usage split, T02 error impls moved)

Everything on `fix-d` is a golden-side change and cherry-picks cleanly onto
the scaffold: the new `tests/t06_usage.rs`, the trimmed
`tests/t07_transcript.rs`, the test moves in `tests/t02_errors.rs`,
`tests/t04_history.rs` and `tests/t05_proc.rs`, the doc comments on
`StatError` and `RecordError` in `src/error.rs`, the five sheets and
`exercises/ANSWERS-golden.md` (golden only; not shipped). What follows is
the scaffold-only work: `todo!` markers, which golden does not carry.

Task numbers are the new ones (`exercises/README.md`, "Numbering changed").
If the scaffold's markers still say `T05` where this file says `T07`, the
renumbering commit has not reached it yet; the *new* text is the same
either way.

## 1. `src/error.rs`

Four marker changes, each inside one impl body. Nothing else in the file
moves: `RecordError`'s three `From` impls keep `todo!("T02")`.

| impl | current text | new text |
|---|---|---|
| `impl fmt::Display for StatError`, body of `fmt` | `todo!("T02")` | `todo!("T05")` |
| `impl Error for StatError`, body of `source` | `todo!("T02")` | `todo!("T05")` |
| `impl fmt::Display for RecordError`, body of `fmt` | `todo!("T02")` | `todo!("T04")` |
| `impl Error for RecordError`, body of `source` | `todo!("T02")` | `todo!("T04")` |

Unchanged, for the record: `UnknownName`, `FrameError` and `ForwardError`
(`Display`, `source`, `From`) and `From<io::Error>`,
`From<serde_json::Error>`, `From<UnknownName>` for `RecordError`, all
`todo!("T02")`.

## 2. `src/usage.rs`

Every marker in the file changes task. Eleven bodies:

| function | current text | new text |
|---|---|---|
| `Usage::from_json` | `todo!("T07")` | `todo!("T06")` |
| `Usage::total` | `todo!("T07")` | `todo!("T06")` |
| `impl Add for Usage`, `add` | `todo!("T07")` | `todo!("T06")` |
| `impl AddAssign for Usage`, `add_assign` | `todo!("T07")` | `todo!("T06")` |
| `impl Sum for Usage`, `sum` | `todo!("T07")` | `todo!("T06")` |
| `response` | `todo!("T07")` | `todo!("T06")` |
| `usage_by_model` | `todo!("T07")` | `todo!("T06")` |
| `Rates::cost_micros` | `todo!("T07 stretch")` | `todo!("T06 stretch")` |
| `parse_price` | `todo!("T07 stretch")` | `todo!("T06 stretch")` |
| `rates_for` | `todo!("T07 stretch")` | `todo!("T06 stretch")` |
| `transcript_cost` | `todo!("T07 stretch")` | `todo!("T06 stretch")` |

One command does both files, from the scaffold root:

```sh
sed -i 's/todo!("T07 stretch")/todo!("T06 stretch")/; s/todo!("T07")/todo!("T06")/' src/usage.rs
```

and, for `src/error.rs`, by hand or with the line numbers `grep -n 'todo!("T02")' src/error.rs`
prints: the fifth and sixth matches are `StatError`'s, the seventh and
eighth are `RecordError`'s `Display` and `source` (the ninth to eleventh
are its `From` impls and stay).

## 3. `src/ingest/claude/transcript.rs`

No marker changes: every body there stays `todo!("T07")`. The usage half
of old T07 lived entirely in `src/usage.rs`.

## 4. Tests

- `tests/t06_usage.rs` is new and is copied whole; it is a provided suite,
  not a learner-written one, so it carries no `#![allow(unused)]` line.
- `tests/t07_transcript.rs`, `tests/t02_errors.rs`, `tests/t04_history.rs`
  and `tests/t05_proc.rs` are provided suites too: copy golden's versions.
- No skeleton under `exercises/skeletons/` changes.

## 5. Check

With every stub intact the scaffold must still compile:

```sh
cargo test --offline --no-run
grep -c 'todo!("T02")' src/error.rs   # 10
grep -c 'todo!("T06' src/usage.rs      # 11
grep -c 'todo!("T07' src/usage.rs      # 0
```
