# Scaffold notes for the T01–T04 fixes

Everything on `fix-a` is a golden-side change. This file lists what the
scaffold needs on top, either because the fix only exists in the scaffold or
because a golden change lands in a file the scaffold carries in a different
form. Everything not listed here cherry-picks cleanly: the four sheets, the
T01/T03 test files, the three new fixtures, and the two hidden suites.

## 1. `tests/t02_forward.rs` (scaffold copy of `exercises/skeletons/tests/t02_forward.rs`)

The scaffold's file is the skeleton plus one line after the module comment:

```rust
// Delete once the tests are written.
#![allow(unused)]
```

Re-copy the skeleton from `fix-a` and re-add that line. The changes it
brings, if you would rather apply them by hand:

- `use std::time::Duration;` becomes `use std::time::{Duration, Instant};`.
- A new constant above `socket`:
  ```rust
  const SILENT_FOR: Duration = Duration::from_secs(5);
  ```
  with its doc comment (in the skeleton).
- `one_shot` reads the first line through a named `reader`, and its `None`
  arm is no longer `thread::sleep(Duration::from_millis(400))` but:
  ```rust
  None => {
      // Wait for a second line that never comes. A client that
      // times out drops its stream, and that ends this read at once.
      stream.set_read_timeout(Some(SILENT_FOR)).unwrap();
      let _ = reader.read_line(&mut String::new());
  }
  ```
- The doc comment on `should_classify_ok_false_as_rejected_with_the_reason`
  names `no reason given`; the one on
  `should_report_io_when_the_daemon_never_answers_within_the_timeout` says
  to take an `Instant` and assert `send` returned well before `SILENT_FOR`,
  and why; the one on `should_surface_a_refusal_from_a_live_daemon` names
  the `daemon refused the frame: <reason>` prefix.

## 2. `tests/t04_tail.rs` (scaffold copy of `exercises/skeletons/tests/t04_tail.rs`)

Same shape: skeleton plus the `#![allow(unused)]` line. Re-copy. The changes:

- A new paragraph at the end of the module comment telling the learner to
  compute expected offsets from the data.
- A helper after `append`:
  ```rust
  /// The length of some bytes as an offset, so an expectation is derived from
  /// the data it describes: `assert_eq!(t.offset(), len(b"one\ntw"))`.
  fn len(bytes: &[u8]) -> u64 {
      bytes.len() as u64
  }
  ```

## 3. `src/names.rs`, unit tests

The scaffold keeps `should_round_trip_every_phase_through_its_name` in the
`#[cfg(test)]` module. It calls `Phase::from_str`, which is `todo!("T02")`
there, so it panics until T02. Delete the whole test and put this comment
where it was, as golden now has:

```rust
    // The `Phase` round trip through `FromStr` is pinned by `t02_errors`,
    // so nothing here needs T02 and the whole module is green after T01.
```

`t02_errors` already pins both halves (round trip, and `"permission"` is an
error), so nothing is lost.

## 4. `src/paths.rs`

Two edits outside any body, identical to golden's:

- The doc comment on `cwd_slug` says "every character" (was "every byte"),
  adds the sentence "Character, not byte: `é` is two bytes and one `char`,
  and it becomes one `-`." and the example line
  `` `/home/u/café`                      -> `-home-u-caf-` ``.
- The unit test `should_replace_every_non_alphanumeric_byte_when_slugging_a_cwd`
  is renamed `should_replace_every_non_alphanumeric_character_when_slugging_a_cwd`.
  Its body is unchanged.

## 5. `src/tail.rs`, the `numbered_lines` stub (scaffold only)

The scaffold has:

```rust
pub fn numbered_lines<R: BufRead>(reader: R) -> impl Iterator<Item = (usize, io::Result<String>)> {
    todo!("T04");
    #[allow(unreachable_code)]
    std::iter::empty()
}
```

This is the same `todo!(); std::iter::empty()` pattern the T05–T09 plan
replaces in T06 and T09. Make it:

```rust
pub fn numbered_lines<R: BufRead>(reader: R) -> impl Iterator<Item = (usize, io::Result<String>)> {
    // The `empty()` below only keeps the `impl Iterator` signature compiling
    // while the body is missing. Replace the whole body, not just the `todo!`.
    todo!("T04");
    #[allow(unreachable_code)]
    std::iter::empty()
}
```

`src/ingest/json.rs` has the same pattern on `array_at` (`todo!("T03")`
then `std::iter::empty()`); give it the same comment with `T03`.

## 6. Doc and module comments to mirror (all outside bodies)

Apply the same hunks as the golden commits; none touches a stubbed body.

- `src/tail.rs`: module comment (split-character hazard; `seek` returns the
  position), `DEFAULT_MAX_LINE` doc (terminator counts), `Polled` field docs
  (blank lines; truncation ends a skip; which poll counts an oversized
  line), `tail_start` doc (all four cases), `numbered_lines` doc (`lines`
  continues after `InvalidData`). Commit `b541d21`.
- `src/error.rs`: the `Display`-versus-`source` paragraph at the end of the
  module comment. Commit `37778cb`.
- `src/model.rs`: the T01 and T02 rows of the header table, and the doc
  comment on `StartSource`. Commit `37778cb`.
- `src/ingest/claude/hook.rs`: two paragraphs added to the module comment
  after "…never on the agent's activity." (the `SubagentStart`/`SubagentStop`
  actor exception; where the fallbacks are listed). Commit `8ae99e7`.

## 7. `exercises/tasks/T03-hook-adapter.md`

The scaffold's copy already lacked the duplicated `wait_kind` bullet that
golden's had. The rewritten sheet also drops it, so a cherry-pick of
`6725ed0` conflicts on those two lines; take the `fix-a` version whole.

## 8. Counts that changed

- `t03_hook`: 31 → 40 tests (sheet updated).
- `t04_tail_golden`: 17 → 20 tests (sheet updated).
- The rendered guide under `~/Guides/`, if it lists test counts or
  checkpoint filters, needs the T03 checkpoint command and both counts.
