# Dry run: T01–T04

Starting commit: `f92b682 fix(test): build the T01 naming agent with discovered, not the reducer`
(branch `dryrun-a`, a clean copy of the scaffold).

Rules I worked under: scaffold, task sheets, `exercises/README.md` and its
reading list, doc comments, compiler and clippy output. No golden, no other
branches. A "cycle" below is one `cargo test` or `cargo check`/`clippy` run.

### T01 — Identifiers and borrowed strings

- **Effort:** 4 cycles (1 baseline run of the failing suite, 1 test run after
  implementing, 1 run of the `names::`/`paths::` unit tests, 1 clippy).
  0 compiler errors of any class; clippy clean on first run. Difficulty
  **1/5**: every body is one to four lines and the tests spell out every
  edge case. Roughly 20% Rust learning (which std method builds a `Box<str>`
  without copying, `OsString::push`), 80% reading: the sheet, the two source
  files, the test file, and the parts of `model.rs` the header table sent me
  to.
- **Prerequisites assumed:**
  - `Box<str>` as a thing you construct, and specifically that
    `String::into_boxed_str` exists and that `Box::from(&str)` allocates. The
    sheet says "must reuse the allocation" without naming the method; a
    learner who has only used `String` has to search for it.
  - `Path::file_name`, `OsStr::to_str`, `Path::with_file_name`,
    `OsString::push`, and that `to_string_lossy` returns a `Cow<str>` (the
    unused `use std::borrow::Cow` in `paths.rs` is the only hint).
  - `Option::filter`, `Option::or` versus `or_else`, `str::strip_prefix`,
    and that a `String` is `FromIterator<char>` so `.chars().map(..).collect()`
    produces one.
  - `format!("{session}.jsonl")` works only because `Display` is written in
    the same task. Nothing says `transcript_path` depends on it; I noticed
    by accident.
  - Macro basics: the sheet does explain `$name` / `$name(x)`, which was
    enough.
  - The reading list sends you to the `model.rs` header table, whose T01 row
    also lists "3. Inputs, the wire enums: `StartSource`, `Origin`,
    `ErrorClass`, `Denier`, alongside `Phase`". T01 never touches those; they
    are T02's `FromStr` targets. I read section 3 for nothing.
- **Sheet accuracy:**
  - Sheet: "Done when `t01_names` passes, and so do the unit tests in
    `names.rs` and `paths.rs`." Actually true only for `paths.rs`. The
    `names.rs` unit test `should_round_trip_every_phase_through_its_name`
    calls `Phase::from_str`, which is `todo!("T02")`, so it panics at the end
    of T01 no matter what you write.
  - Sheet: "`cwd_slug`: every char that is not ASCII alphanumeric becomes
    `-`." The doc comment on `cwd_slug` and the unit test name both say
    "every **byte**". For a valid multibyte character (`é` is two bytes)
    byte-wise gives `--` and char-wise gives `-`. No test distinguishes them.
    I followed the sheet (char-wise, via `to_string_lossy`), and I do not know
    which one Claude's real project directory naming does.
  - `From<String>` "must reuse the allocation": `into_boxed_str` calls
    `shrink_to_fit`, so it reuses the allocation only when the `String` has no
    spare capacity. Nothing in the tests can observe this either way, which
    makes the instruction a comment rather than a requirement.
  - The test `should_prefer_title_then_workspace_then_cwd_when_naming_an_agent`
    depends on `switchboard::reduce::discovered` and `common::row`. The test
    comment does say it is provided, which was enough, but the sheet itself
    never mentions that `paths.rs` is tested through the reducer module.
- **Where I got stuck:** nowhere. Two hesitations, each under a minute:
  whether `Box::from(s)` or `s.into()` is the idiomatic `From<&str>` body
  (either compiles), and whether the three `Option<&str>` arms in
  `display_name` would unify their lifetimes through `or_else` (they do, all
  borrow `agent`).
- **What this drilled:** `as_deref` on `Option<Box<str>>`; writing the same
  `&self.0` body for `as_str`, `Deref`, `AsRef`, `Borrow`, which teaches the
  *names* of the four traits but not their differences (the kata does that);
  `OsString` path surgery in `temp_sibling`. Incidental: macro_rules syntax
  (the sheet's scratch-module escape hatch was not needed), the
  `discovered` constructor, and the unrelated `model.rs` sections the header
  table pointed at.
- **Ordering:** correct as the first task. Inside it, the four identical
  trait bodies could be collapsed: give `as_str` and have the learner write
  the four trait impls *in terms of it*, or ask them to add the `Borrow` impl
  only after seeing the `map.get("a1")` test fail to compile, so the trait's
  purpose is felt rather than told. `display_name` and `temp_sibling` are the
  only bodies that require a decision and would survive being the whole task.
- **Confidence after:** a learner would feel done, and would be right about
  deref coercion and `as_deref`. Shaky: why `Borrow` has a contract and
  `AsRef` does not (only the kata asks; nothing in the code fails if you
  confuse them), and what `Cow<str>` is, since `to_string_lossy` is used
  without ever naming its return type.
- **Suggestions:**
  1. Change "Done when" to: "`t01_names` passes and `cargo test --lib paths::`
     passes; one `names::` unit test stays red until T02." Or move
     `should_round_trip_every_phase_through_its_name` into the T02 test file.
  2. Reconcile byte versus char in `cwd_slug`: pick one, fix the doc comment
     and the unit test name to match, and add a case with a valid multibyte
     character (`/home/u/café`) to `t01_names` so the choice is pinned.
  3. Remove "3. Inputs, the wire enums" from the T01 row of the `model.rs`
     header table, or move it to T02 where `FromStr` for those enums lives.
  4. Name `String::into_boxed_str` in the sheet, or turn "must reuse the
     allocation" into a question: "which std method converts without
     copying, and when does it still reallocate?"
  5. Make the `Borrow` requirement discoverable. Deleting the impl from the
     scaffold is not an option (I first thought it was, then checked:
     `tests/common/mod.rs` calls `tools.get(call)` with a `&str` in
     `gate_of`, and every test file compiles `common`). So instead point the
     sheet at that line: "`gate_of` in `tests/common/mod.rs` is the live
     reason `Borrow<str>` exists; comment the impl out and read the error."
