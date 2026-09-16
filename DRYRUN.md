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

### T02 — Error architecture

- **Effort:** 7 cycles (1 run of my new `t02_forward` tests against the
  stubs, all ten failing on `todo!`; 1 run of both T02 files after
  implementing, all green; 1 clippy; 1 regression run of the `names::`,
  `paths::` and `frame::` unit tests; 1 rerun of `t01_names`; 1 clippy after
  the fix; 1 rerun of `t02_forward`). 0 compiler errors. 1 clippy warning,
  in my own test (`io::Error::new(ErrorKind::Other, ..)` should be
  `io::Error::other`). Difficulty **2/5**: the volume is the cost, not any
  single line. About 30% Rust (the `transpose` chain, the
  `Option<&(dyn Error + 'static)>` return type, `UnixStream` timeouts,
  tuple-variant constructors as `map_err` arguments), 70% deciding and
  reading: message wording for the unpinned variants, what an absent reason
  should say, what a hang-up before any byte should classify as, and what
  the wire names for `StartSource` are.
- **Prerequisites assumed:**
  - `UnixStream` has no connect timeout, `set_read_timeout` exists and must
    be called or a silent daemon blocks forever, `&UnixStream` implements
    `Read` so `BufReader::new(&stream)` borrows rather than moves, and
    `read_line` returning `Ok(0)` means EOF. The sheet says "every I/O step
    maps its error" but never says a timeout has to be *installed*.
  - That `Some(e)` where `e: &serde_json::Error` unsize-coerces to
    `Option<&dyn Error>` without an explicit cast.
  - That `serde_json::Value` implements `Display` (infallible), so the frame
    can be serialised without going through a `Result`. The obvious
    alternative, `serde_json::to_vec(frame)?`, compiles and silently routes a
    serialisation failure into `ForwardError::Malformed`, whose doc says "the
    reply is not JSON". Nothing in the sheet warns about this trap.
  - The `Denier` wire vocabulary is in the doc comment on `Denier` in
    `model.rs`, which the T01 reading covered. The `StartSource` vocabulary
    is nowhere: the tests pin `clear` and `resume`, the fixtures show
    `startup`, and `compact` appears in no file in the repository. I guessed
    it from the variant name.
  - `let ... else` (the given `t02_errors` uses it). Fine for 2024 edition,
    but it is introduced by example, not by the sheet.
- **Sheet accuracy:**
  - Sheet: "The test file pins the exact wording of several messages." True
    for five `FrameError`/`UnknownName` messages. `StatError` and
    `RecordError` wording is unconstrained except `contains("starttime")`,
    so two of the five `Display` impls are written blind. Not wrong, but the
    sheet implies more guidance than exists.
  - Sheet: "For `send` you need something to talk to. A `UnixListener` on a
    thread that reads one line and writes a canned reply is enough, and a
    second variant that never replies covers the timeout." The scaffold
    already provides exactly that (`socket`, `one_shot`). The paragraph
    reads as an instruction to build something that is already built.
  - The provided `one_shot(path, None)` sleeps 400 ms and then *returns*,
    which drops the stream and delivers EOF. A `send` that never sets a read
    timeout gets `Ok(0)` from `read_line` at 400 ms, and if it maps that to
    `Io` (as mine does), the timeout test passes without any timeout
    existing. I only caught this because I asserted `elapsed < 350 ms`. The
    test doc comment ("`Io`, after the timeout") does not say to check
    elapsed time, so a learner can pass the test with the wrong code.
  - Test doc: "When `error` is absent, the reason is still present and says
    so." "Says so" pins nothing; I wrote "daemon gave no reason". If the
    hidden golden pins the text, that is unwinnable from here.
  - `FrameError::Json` must display as `malformed JSON: ` followed by
    something, *and* expose the serde error as `source()`. That is the
    duplicated-source pattern most error-handling guides warn against.
    Neither the sheet nor `error.rs` states a policy, so I duplicated
    consistently across all five enums; a learner who read the guides would
    do the opposite and fail the prefix test on the other variants only if
    they happened to be pinned. The course should say which it wants.
  - The T01 sheet's "Done when" (unit tests in `names.rs` pass) becomes
    true here, once `Phase: FromStr` exists.
- **Where I got stuck:** never blocked. Three decisions with no guidance:
  (1) EOF before any reply: I made it `Io(UnexpectedEof)` and wrote a test
  for it, on the reasoning that no reply ever arrived so "malformed reply"
  would be a lie; (2) `{"pid": null}`: wrong type, not absent, by analogy
  with the given `session_id: null` case; (3) whether `{"ok":false,
  "error":42}` is a reason: no, treated like absent.
- **What this drilled:** `From` + `?` (genuinely: the test
  `should_convert_with_question_mark_into_a_record_error` shows the three
  conversions working in one function), `transpose` (the sheet's "write the
  type after each step" is the best instruction in the first two tasks),
  per-step `map_err(ForwardError::Connect)` with a variant as a function,
  the `Error::source` signature. Incidental: five near-identical `Display`
  matches and three trivial `From` bodies, choosing wording, and the
  socket-test mechanics (provided, and mostly fine).
- **Ordering:** right place. Inside it, `StatError` (used in T06) and
  `RecordError` (used in T04) are written here without a consumer, so their
  `Display` is wording chosen in a vacuum. Moving each enum's `Display` and
  `source` to the task that first raises it would cut this task by a third
  and put each error next to its use; `RecordError`'s three `From` impls
  should stay, because the T02 test exercises them.
- **Confidence after:** solid on `?`, `From`, `Display`, `source`, and on
  why `ForwardError` refuses `From<io::Error>`. Shaky: the std blanket
  `From<E> for Box<dyn Error>` (kata only, never seen in code), `ok_or`
  versus `ok_or_else` (kata only), and read timeouts if their timeout test
  passed for the wrong reason.
- **Suggestions:**
  1. Make `one_shot(path, None)` sleep well past any plausible timeout (5 s)
     or have the test doc say "assert it returned before the daemon's
     hang-up", so a `send` with no `set_read_timeout` fails the test.
  2. Add the `StartSource` wire vocabulary (`startup`, `resume`, `clear`,
     `compact`) as a doc comment on the enum in `model.rs`, matching the one
     `Denier` already has, or list it in the sheet.
  3. State the Display-versus-source policy in the `error.rs` module
     comment. If the intent is "Display includes the cause", say so; if not,
     change the pinned prefix test to accept `malformed JSON` alone.
  4. In the sheet's `client.rs` bullet add: "`Value` implements `Display`;
     do not reach for `serde_json::to_vec(frame)?`, and say which variant
     that `?` would land in."
  5. Either pin the absent-reason text or say explicitly that the golden
     checks only that it is non-empty.
  6. Delete the "you need something to talk to" paragraph or rewrite it as
     "the helpers `socket` and `one_shot` are provided; read them and notice
     what `None` does after 400 ms".
