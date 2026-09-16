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

### T03 — JSON navigation and the hook adapter

- **Effort:** 4 cycles (1 run of `t03_hook`, 31 of 31 green on the first
  build; 1 clippy, clean; 1 run of the `json::`, `hash::`, `forward::`
  unit tests; 1 regression run of T01 and T02). 0 compiler errors, 0 clippy
  warnings. Difficulty **3/5**: no single line is hard, but the surface is
  three modules, fourteen hook arms, twenty-one fixtures, and about six
  product decisions that neither the sheet nor the tests make. Roughly 25%
  Rust (`try_fold` with an `Option` accumulator, `Option<&Vec<_>>` as an
  iterator, `bool::then` under the no-`match` rule, `zip`, a closure that
  captures `frame` and `ctx` to build events), 75% reading and deciding:
  the mapping table, every fixture, survey 14, and `docs/03`, which the
  reading list requires before this task and which contributed nothing to it.
- **Prerequisites assumed:**
  - `serde_json::Value::get` takes any `Index`, including `&&str`, which is
    what the `try_fold` closure receives from `path.iter()`. Not in any
    listed reading.
  - `Option::zip`, `bool::then`, `Option::flatten`, `Iterator::try_fold`
    with `Option` as the short-circuit type. The sheet names `try_fold` and
    `zip`; the no-`match` rule in `hash.rs` forces `then` or
    `Option::filter` for `from_hex`, and neither is named.
  - `u64::to_be_bytes` / `from_be_bytes`, `wrapping_mul`, `{:016x}`.
  - `serde_json::Map`: `remove` returns the owned value, keys are `String`,
    `"hook".into()` makes a `Value`. `match raw { Value::Object(m) => m,
    _ => Map::new() }` to take the map out by value.
  - `std::iter::once(event).chain(option)` for "one event plus maybe a
    snapshot", relying on `Option: IntoIterator`.
  - `&*frame.event` to match a `Box<str>` against literals.
  - Domain: which hooks exist and which fields each carries. The README's
    reference list names survey 14 for T03; the T03 sheet itself never
    points at it. I found the field table by following the README, not the
    sheet.
- **Sheet accuracy:**
  - Sheet: "`t03_hook` passes: 30 tests". The file has 31.
  - `hook.rs` module comment: "Every event's actor is the subagent
    `agent_id` names, when present." The test
    `should_attribute_subagent_lifecycle_to_the_agent_and_name_the_child`
    requires `Actor::Agent` for `SubagentStart` and `SubagentStop`, both of
    whose fixtures carry `agent_id`. The rule has an exception the comment
    does not state. I caught it by reading the test before writing, not by
    failing it.
  - Reading list: "`docs/03-types-and-transitions.md` before T03: which
    harness signal drives each transition." That file is 587 lines; half
    is Codex, app-server and statusline material, its type sketches use
    names the model has since replaced (`AgentId`, `RootKey`, `Leveled`,
    `EdgeKind`), and its transition sections (§4, §13) are reducer
    material for T07 and T08. The one thing T03 needs, the hook-to-event
    table, is in the `hook.rs` module comment already. This is the
    biggest misdirect in T01 to T04: twenty minutes of reading that
    changed nothing I wrote.
  - Decisions with no test and no guidance, and what I chose:
    `SubagentStop` has no outcome field (chose `Completed`);
    `PermissionDenied.reason` is an open string per survey 14, not the
    `toolDenialKind` vocabulary `Denier: FromStr` parses (chose: parse,
    fall back to `AutoMode` because the survey says the event fires on
    auto-mode denials); `Elicitation` has no `tool_name` (chose
    `mcp_server_name`, else `"Elicitation"`); `SessionEnd` without
    `reason` (chose `"unknown"` rather than dropping the event);
    `SessionStart` with an unrecognised `source` (chose to drop, since
    `source` is not optional on the event and guessing `Startup` would be a
    claim); `agent_type` to `SubagentKind` (every captured value is
    `general-purpose`; chose `Task` with guessed spellings for the other
    two); which `background_tasks[].status` values are live (the test
    only has `completed`; chose to drop `completed`, `failed`, `killed`);
    `PreToolUse` with no `tool_input` at all (chose to drop, since
    `ToolDispatched.hash` is not optional). Eight decisions. A learner
    cannot know whether any of them match the golden until grading.
  - Sheet: "`frame_hash` ... one chain with `or_else`" and
    "`call_and_tool` is a single `zip`": both accurate and both good.
  - The `from_hex` trap the sheet promises ("the test has a trap in it")
    is real but arrives from two directions: `"ééééééé00"` has byte length
    16, so a length check alone passes it; `"+f00..."` is the
    `from_str_radix` trap the doc comment already explains. The sheet
    points only at the second.
- **Where I got stuck:** not on Rust. Time went to the eight decisions
  above, to reading `docs/03` for nothing, and to noticing the lifecycle
  actor exception. The one Rust wrinkle was `from_hex` without `match` or
  `if`, solved with `then(..).flatten()`.
- **What this drilled:** `Option` flow, for real: `str_at` into `parse`
  into `map_or`, `zip`, `or_else`, `try_fold`, `into_iter().flatten()`;
  `let ... else`; variant constructors as functions; borrowed returns with
  an explicit lifetime (`str_at`). Incidental: twenty-one fixtures, the
  hook vocabulary, `Map` surgery in `scrub`, and the eight product
  decisions.
- **Ordering:** after T02 is right; the end-to-end tests need
  `decode_request` and `scrub`. Inside, the task is two tasks: `json.rs` +
  `hash.rs` + `scrub` is an hour of pure combinators with tests that
  fully specify them; `hook.rs` is an hour of domain mapping where the
  tests specify half. Splitting them would make the T02 to T03 jump, the
  first real one in the range, a step instead of a wall.
- **Confidence after:** good on `Option` chains and `let-else`, and would
  feel it. Shaky: `try_fold` (one use, could be memorised rather than
  understood), why `Option<&Vec<_>>` iterates, and whether their eight
  guesses are right, which they have no way to check.
- **Suggestions:**
  1. Remove `docs/03` from the pre-T03 reading and move it to before T07.
     Replace it with "survey 14 §1.1 (the field table) and the `hook.rs`
     module comment".
  2. Fix "30 tests" to 31. Add to the `hook.rs` module comment: "except
     `SubagentStart` and `SubagentStop`, where `agent_id` names the child
     and the actor is the agent that spawned it".
  3. Add a "decisions the tests do not make" block to the sheet listing
     the eight cases above with the intended answer for each, or add a
     fixture and a test per case. Either fixes the same problem; the
     second is better pedagogy.
  4. Next to the no-`match` constraint, name `bool::then` and
     `Option::filter` as the tools for `from_hex`.
  5. Split the sheet into T03a (helpers, hash, scrub; `cargo test --test
     t03_hook -- should_read should_walk should_iterate should_return
     should_hash should_round should_drop should_fall should_still`) and
     T03b (the adapter), or at least put a "checkpoint" line between them.
  6. Add a test for `Stop` with `"background_tasks": []` and no crons:
     the sheet says `Some(vec![])` and `None` differ and the reducer
     cares, but no test pins the `Some(vec![])` side.

### T04 — Line-oriented I/O: tail and history

- **Effort:** 6 cycles (1 run of my `t04_tail` tests against the stubs, 21
  of 21 failing on `todo!`; 1 run after implementing: `t04_history` 8 of 8
  green, `t04_tail` 18 of 21, and all three failures were wrong byte
  counts in *my tests*; 1 rerun, all green; 1 clippy, clean; 1 `--lib`
  regression that only surfaced the T05/T06 stubs; 1 regression of T01 to
  T03). 0 compiler errors. 0 clippy warnings on the build, though I
  rewrote one `== false` before compiling because I knew clippy would
  object. Difficulty **3/5**: `Tail::poll` is the first function in the
  range that has to hold state across calls (carry, skipping, offset,
  truncation) and the first where a wrong test looks exactly like a wrong
  implementation. About half Rust (`Read + Seek` generics, the `&mut R`
  reborrow through `take`, `BufReader` + `read_until`, what `lines()` does
  after `InvalidData`, `mem::take`, a `W: Write` that is sometimes
  `&mut File` and sometimes `File`), half deciding tail semantics the sheet
  leaves open.
- **Prerequisites assumed:**
  - `Seek::seek` returns the new position, so `seek(SeekFrom::End(0))` is
    how you learn the file length. Both `poll` and `tail_start` need it and
    nothing says it.
  - `read_until` includes the delimiter in the buffer and returns a
    fragment with no delimiter at EOF: that is the whole partial-line
    mechanism, and it is only implied by "use `read_until`".
  - `BufRead::lines` consumes the bad bytes and *keeps going* after an
    `InvalidData` error. `read_day`'s spec ("an `InvalidData` read error is
    a bad line; any other I/O error ends the read") and `numbered_lines`
    both depend on it. The provided test
    `should_keep_reading_after_a_line_of_invalid_utf8` pins it, which is
    good, but the sheet does not name it as a fact to know.
  - `std::mem::take`, `Option::is_ok_and`, `String::from_utf8(Vec<u8>)`,
    `Duration::as_millis` returning `u128`.
  - `serde_json::json!` plus index assignment `v["k"] = json!(..)`.
- **Sheet accuracy:**
  - Sheet: "Use `read_until(b'\n')` on a `BufReader` wrapped around
    `file.take(...)`." Accurate, and the single most useful hint in the
    four sheets: it fixes the shape of `poll` and sets up kata 1.
  - Sheet bullet: "`tail_start` when the window ... holds no newline at
    all". The expected answer is not stated anywhere. I return the file
    length (nothing in the window is a whole line), and wrote a test
    pinning that, so the hidden suite may disagree.
  - `Tail::starting_at` doc: "which must be the start of a line". Combined
    with my `tail_start` this has a hole: a file whose last line is still
    being written gets a tail started at its end, and when the rest of that
    line arrives, the fragment is yielded as a line. Neither doc addresses
    the file-ends-mid-line case. I accepted it because JSONL writers write
    whole lines, and noted it in the test.
  - Sheet: "blank lines, which are skipped but not counted as skipped" for
    `Tail`; the `numbered_lines` doc says "blank and whitespace-only". Are
    whitespace-only lines blank for `Tail`? I said yes, and pinned it.
  - `DEFAULT_MAX_LINE` doc: "Lines longer than this are skipped." With or
    without the `\n`? I chose content length, so a line of exactly
    `max_line` bytes is kept, and pinned that too.
  - Sheet: "an oversized line spanning two polls, which is skipped whole
    and counted once". Counted in which poll? I count it the moment the
    carry exceeds the limit, which means the count lands in an earlier
    poll than the newline. My test sums `skipped` across polls so either
    convention passes; a golden test that checks a single poll would not.
  - The history side is the best-specified code in T01 to T04: the mixed
    fixture and `should_skip_foreign_lines_and_report_bad_ones_with_line_numbers`
    pin every error variant, its line number and the check order (type
    before ts before pid). Nothing to decide.
  - Kata 1's `BufReader::new(file.take(n))` is exactly the line `poll`
    needs, so the kata is answered by having done the task. Good.
- **Where I got stuck:** one cycle lost to my own arithmetic: three
  expected offsets were each off by one (I miscounted `b"done\n{\"a\":"`
  as 11 bytes, and two others). The implementation was right. This is a
  genuine learner hazard in a test-first task built on byte offsets: the
  failure message is indistinguishable from an implementation bug, and a
  learner who trusts their test would "fix" working code. Two decisions I
  had to make alone: what happens when the file shrinks while `skipping`
  is set (I reset it and wrote a test), and whether a UTF-8 character
  split across two polls must reassemble (it must; carrying bytes rather
  than `String` makes that free, and I wrote a test so the golden cannot
  quietly require the other thing).
- **What this drilled:** `R: Read + Seek` and `W: Write` generics with
  `Cursor<Vec<u8>>`, `Vec<u8>`, `&mut File` and `File` as the concrete
  types (the provided history test that borrows in a loop and then moves
  is the clearest demonstration of ownership-through-generics in the
  range); `BufReader` + `read_until`; `lines()` error semantics; carrying
  bytes across calls. Incidental: hand-counting bytes, six tail-semantics
  decisions, `json!` assembly.
- **Ordering:** after T03 is right (`history.rs` uses `str_at`/`u64_at`
  and T02's `RecordError` and `Phase: FromStr`). Inside the task, the
  sheet lists `tail.rs` first and `history.rs` second; the reverse is a
  better ramp. History is fully specified and exercises `Write`/`BufRead`
  generics on a warm-up scale; `Tail` is the hardest single function in
  T01 to T04 and should come once the reader/writer generics are already
  comfortable.
- **Confidence after:** solid on `Read`/`BufRead`/`Write` generics and
  test doubles, and would feel it. Shaky: whether their `Tail` agrees with
  the golden's 17 tests on the unspecified edges (whitespace-only lines,
  the `max_line` boundary, `tail_start` with no newline, truncation while
  skipping, which poll counts a spanning skip), and `Seek` if they did not
  already know `seek` returns the position.
- **Suggestions:**
  1. State the expected result for each `tail_start` bullet, especially
     "holds no newline at all", or say explicitly "you decide; write the
     reason in the test's doc comment and I will grade the reasoning".
  2. State whether `max_line` counts the terminator, whether
     whitespace-only lines are blank for `Tail`, and in which poll a
     spanning oversized line is counted.
  3. Tell the learner to derive expected offsets from the data (`let data
     = b"..."; data.len()`) rather than counting by hand, or give the
     skeleton a `fn len(bytes: &[u8]) -> u64` helper with a comment saying
     why. All three of my failures were counting errors.
  4. Swap the order in the sheet: `history.rs` first, `tail.rs` second.
  5. Add to the hazards list: "`BufRead::lines` consumes the bytes of an
     invalid line and continues; `read_day` relies on that" and
     "`seek(SeekFrom::End(0))` returns the length".
  6. Add a bullet for a multibyte character split across polls; it is the
     one hazard that forces carrying bytes rather than text, and nothing
     in the sheet or module comment mentions it.

## Overall

**Shape of the ramp.** T01 is a 1, T02 a 2, T03 and T04 both 3. Cycles were
4, 7, 4 and 6; compiler errors were zero on every task and clippy fired once,
on my own test. That flat error count says more about the scaffold than
about the tasks: the signatures are all given, so the borrow checker and
type inference have very little room to bite in T01 to T04, and the
"ownership in real APIs" gap the README lists is not reached until T07. The
two jumps are T02 to T03, which is a jump in *surface* (three modules,
fourteen event arms, twenty-one fixtures, eight product decisions with no
tests), and T03 to T04, which is flat in difficulty but changes *kind*, from
pure combinators to a stateful reader plus a test-first suite built on byte
offsets. Nothing in the range is too hard. The problem is not steepness but
footing: too much of the difficulty is finding out what was meant.

**The three biggest pedagogical problems.**

1. **Underspecification graded by a hidden suite.** T03 has eight
   adapter decisions and T04 has five or six tail-semantics decisions that
   no test pins and no sheet line settles, yet a golden suite will grade
   them. A learner cannot tell "I am wrong" from "I chose differently", and
   the course promises the golden "exists to find the edge cases your tests
   missed, not to replace them". Either list the intended answers in the
   sheet or add a fixture and test per case. The history half of T04 shows
   what fully specified looks like, and it was the most pleasant code to
   write in the range.
2. **Reading that does not pay.** The reading list sends the learner to
   `docs/03` before T03 (587 lines, nothing the adapter needs, type names
   the model has since replaced) and the `model.rs` header table sends the
   T01 learner to "the wire enums" that T02 uses. Meanwhile the material
   that *is* needed is unreferenced or absent: survey 14's field table is
   named only in the README, `Denier`'s wire vocabulary is in a doc comment
   the sheet never points at, and `StartSource`'s vocabulary is not in the
   repository at all (`compact` appears nowhere but my code). A learner
   who follows the list reads the wrong things and then greps for the right
   ones.
3. **Sheets and tests that can be satisfied for the wrong reason, or not
   at all.** T01's "Done when" cannot be met (a `names.rs` unit test needs
   T02). T02's silent-daemon helper hangs up at 400 ms, so a `send` with no
   read timeout passes the timeout test. T02's sheet tells you to build a
   socket harness the skeleton already provides. T03 says 30 tests; there
   are 31. `hook.rs` says "every event's actor is the subagent" and a test
   requires the opposite for lifecycle events. Each is small; together they
   teach the learner to distrust the sheet, and the sheet is what they lean
   on when stuck.

**Recommended restructuring for T01 to T04.**

- **T01**: keep as the opener. Fix "Done when", settle byte versus char in
  `cwd_slug` with a pinned test, name `into_boxed_str`, and correct the
  `model.rs` header row.
- **T02**: keep the position. Move `StatError`'s and `RecordError`'s
  `Display`/`source` to T06 and T04 respectively, where they have a
  consumer (keep `RecordError`'s `From` impls, which T02 tests). Make the
  silent daemon sleep past any timeout. Put the `StartSource` vocabulary in
  a doc comment. State the Display-versus-source policy.
- **T03**: split into T03a (json helpers, hash, scrub: pure combinators,
  fully specified by tests, about an hour) and T03b (the adapter, with a
  "decisions the tests do not make" block or a fixture per decision).
  Replace `docs/03` in the pre-reading with survey 14 §1.1 and the
  `hook.rs` module comment; move `docs/03` to before T07.
- **T04**: swap the order to history then tail. State the expected answers
  for the tail edges. Add the split-multibyte hazard. Tell the learner to
  compute offsets from the data.
- **Across the range**: the katas are consistently the best part. T02's
  "write the type after each step" and T04's kata 1 (which is literally the
  line `poll` needs) turn the code into the lesson. More of the sheets
  should work that way: a question whose answer is the next line you have
  to write, rather than a rule to follow.
