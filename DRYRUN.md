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
