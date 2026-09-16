# Dry run: T10–T13

Starting commit: `4e08a64` (branch `dryrun-c`, T01–T09 pre-filled).

A note on the starting state before T10: the three learner-written test
files (`t02_forward`, `t04_tail`, `t07_turns`) are still `todo!("write this
test")` in this copy, so the full suite reports 44 failures that have nothing
to do with T10–T12. A learner who had really finished T09 would have written
them. I treated those files as out of scope and used the other suites as the
regression baseline: 42 unit tests plus t01, t02_errors, t03, t04_history,
t05, t06, t08, t09 all green at the start.

Tally convention: every `cargo test`, `cargo check`, `cargo clippy` run is one
cycle. A "probe" is a cycle I ran deliberately to see what a wrong version
says, after the task already passed; probes are counted separately.

### T10 — async I: the core actor

- **Effort:** 2 cycles (baseline run against the stub, then the first real
  attempt, which passed) plus 1 clippy, plus 1 probe. Zero compiler errors on
  the real attempt. Difficulty 2/5: the function is about twenty-five lines
  and every non-obvious piece is signposted by an import the scaffold already
  carries (`Instant as TokioInstant`, `std::future`, `sleep_until`). Roughly
  70% of the time went to reading (`daemon/mod.rs`, `reduce/mod.rs`,
  `reduce/tick.rs`, the test file) and 30% to writing Rust. Caveat on the
  measurement: I already knew `select!`, `watch` and `pending()`, so the
  cycle count says more about the sheet's signposting than about how hard a
  first-time tokio user would find it. The places a first-timer would burn
  cycles are listed under "Where I got stuck".
- **Prerequisites assumed:**
  - `tokio::select!` syntax (`pat = fut => expr`, the `() = wake` pattern, a
    `break` inside an arm inside a `loop`). Nothing before T10 uses it and the
    sheet names it as a gap but does not point at the tokio docs page or
    give a two-line example.
  - **Cancellation safety.** The loop only works because
    `mpsc::Receiver::recv` is cancel-safe: when `wake` wins, the pending
    `recv` future is dropped and no message is lost. The sheet does not
    mention the concept. A learner who does not know it either trusts the
    loop blindly or worries about a bug that does not exist. This is the
    single most important async fact in T10 and it is absent.
  - The two `Instant` types. `next_deadline` returns `std::time::Instant`,
    `sleep_until` takes `tokio::time::Instant`. The alias import is the only
    hint that `from_std` exists.
  - `watch::Sender::send_if_modified` and its closure contract (`&mut T`,
    return `bool`). Named on the sheet, not explained. Fine for a learner who
    reads docs.rs, but this is the first `watch` use in the course.
  - `Arc<View>` as the watch payload, and therefore a double deref in the
    comparison. See the probe below.
  - `start_paused` and auto-advance. The test file header explains this well;
    the sheet does not say "read the test header first", and it should,
    because the two time-only tests are otherwise mysterious.
- **Sheet accuracy:** good. Every named function exists with the named
  behaviour. Small gaps rather than errors:
  - "`dispatch`: `PublishState` goes to `publish`. Everything else goes to the
    runner." True, but the sheet does not say that `reduce::Effects` already
    guarantees at most one `PublishState`, last. Knowing that removes a
    question the learner otherwise has to answer by reading `reduce/mod.rs`
    (do I need to dedupe? does order matter?).
  - "including the two tests where the only thing that happens is time
    passing". Both of those tests send three or four inputs first and then
    only time passes. Accurate in spirit, slightly misleading literally.
  - The sheet says "Use `crate::daemon::now()`, never `Instant::now()`", but
    the `now()` call is only needed in the tick arm. Worth saying so: a
    learner may hunt for a second place to use it.
  - `core::run` (the spawned body) is provided and just calls
    `run_until_closed`. The sheet lists only the three functions to write,
    which matches.
- **Where I got stuck:** nowhere, on the real attempt. Predicted sticking
  points, one of which I probed:
  - Probe: writing `*current == rendered` inside `send_if_modified` gives
    `E0308 expected Arc<View>, found View`, and rustc's suggested fix is
    `rendered.into()`, which compiles and is wrong-ish (it allocates a fresh
    `Arc` just to compare, then you allocate again to store). The right
    answers are `**current == rendered` or `current.as_ref() == &rendered`.
    This is a real Deref lesson hiding in T10 and it deserves a kata; the
    compiler's hint actively leads away from it.
  - If a learner writes `next_deadline(&board)` inside the `async` block
    instead of computing it into a local first, the future borrows the board
    immutably across the `select!` and the `apply(&mut board, ..)` arm fails
    to borrow mutably. Kata 2 is about exactly this, but the sheet's bullet
    ("recompute `next_deadline(&board)` and build a `wake` future") could be
    read as "call it inside". A one-word tweak fixes this (see Suggestions).
  - `wake` declared outside the `loop` is a use-after-move on the second
    iteration. Kata 1 covers it.
- **What this drilled:** `select!` as a two-way wait, a future as a value
  built fresh each pass, ownership of the board by one task, `watch` as
  latest-value state versus `mpsc` as a stream, `send_if_modified` for
  change-only wake-ups, and one Deref lesson via `Arc<View>`. Incidental
  complexity: none of note. The domain (what a dwell is, what
  `TURN_STALE` does) is already known from T09, and the reducer is a black
  box with three entry points. This is the smoothest task in the range.
- **Ordering:** T10 is the right first async task. It depends on T09 only
  for `next_deadline` and `tick`, and the test harness shows the learner how
  the daemon will be wired before they write any I/O. Nothing inside it
  should be split. If anything, it is slightly too small: after T09's
  1h-of-reducer work, this is a breather rather than a step up.
- **Confidence after:** a learner would feel they understand `select!` and
  `watch`, and they would be right about the shapes. They would still be
  shaky on cancellation safety (never mentioned), on why the effects channel
  is unbounded while the inbox is bounded (mentioned in `daemon/mod.rs` only
  by its type), and on what `start_paused` actually does to a `sleep` in
  their own code versus the test's. None of those gaps bite in T10; the
  first two bite in T12.
- **Suggestions:**
  1. Add to the sheet, under the `select!` bullet: "`Receiver::recv` is
     cancel-safe: if the deadline wins, the dropped `recv` future has not
     taken a message. That is what makes it legal to race the two. Read
     the 'Cancellation safety' section of the `tokio::select!` docs before
     writing the loop." Then add kata 4: "Which futures in this loop are
     cancelled when the other arm wins, and why is that safe?"
  2. Change the first bullet to "Each pass, compute `next_deadline(&board)`
     **into a local**, then build a `wake` future that captures it." The
     current wording invites calling it inside the `async` block.
  3. Add a kata on `Arc<View>`: "Inside `send_if_modified`, `*current ==
     rendered` does not compile. Write the two correct spellings, and say
     why rustc's suggested `.into()` compiles but is the wrong fix."
  4. Say "Read the header of `tests/t10_core.rs` first" next to "Read
     `src/daemon/mod.rs` first". The paused-time explanation lives there.
  5. Note in the sheet that `reduce::Effects` already puts at most one
     `PublishState` at the end of the vector, so `dispatch` is a plain
     for-loop with no dedupe and no ordering concern.
  6. Mention that `from_std` exists (`TokioInstant::from_std(at)`), or
     leave the alias import as the hint but say "the alias is a hint".

### T11 — async II: the socket

- **Effort:** 1 real cycle (passed on the first attempt), 1 clippy, 1 full
  suite, plus 2 probes. Zero compiler errors on the real attempt. Difficulty
  3/5 for a learner meeting this cold: five functions, two of which
  (`handle`, `resolve`) each carry a distinct ownership lesson, and the
  compile errors that lesson produces are reported far from where they are
  caused. About 60% of my time was reading (the frame module, the two
  adapter signatures, `proc::ancestors`, the test helpers `FakeProc` and
  `board_with`) and 40% writing. Same caveat as T10 on the measurement: I
  knew the tokio APIs. The probes below are what I expect a first-timer to
  hit.
- **Prerequisites assumed:**
  - `tokio::io::Lines` is not an iterator. The sheet says "loop over
    `lines()`", and a learner coming from T04's `BufRead::lines()` will write
    `for line in lines` or `while let Some(l) = lines.next().await` (there is
    no `StreamExt` in the dependency set). The working form is
    `while let Ok(Some(line)) = lines.next_line().await`. Nothing in the
    course says this.
  - `AsyncReadExt::take` producing a `Take<R>` that must then be wrapped in
    `BufReader` before `.lines()` exists. The imports hint at `BufReader`,
    but the composition order (`BufReader::new(read.take(n)).lines()`) is
    something the learner has to discover.
  - `into_split` returning owned halves, and why owned matters (the spawned
    future must be `'static`). The gap list names `into_split`; nothing
    explains the `split` versus `into_split` difference.
  - `Arc<Path>` from a `PathBuf` (`Arc::from(path_buf)`) and from a `&Path`
    (`Arc::from(path)`, seen in the test). Small, but the first `Arc<Path>`
    in the course.
  - `mpsc::error::TrySendError` has two variants and they mean different
    things (full: the core is behind; closed: the core is gone). The sheet
    says "log on a full inbox" without naming the type.
  - `JoinHandle<Option<T>>` from `spawn_blocking` awaits to
    `Result<Option<T>, JoinError>`, so the tail is `.ok().flatten()`. A
    combinator lesson the sheet does not mention.
  - `claude::statusline` takes an `AccountId` the sheet never mentions.
    `default_account` is imported in the stub, which is the only hint.
- **Sheet accuracy:**
  - "Cap the connection with `read.take(MAX_CONNECTION_BYTES)`, then loop
    over `lines()`." Correct API, misleading verb: see above.
  - "`Hook` and `Statusline` get resolved to a process, run through the
    adapter, `offer`ed to the inbox, then `ok_reply`." True. The
    `Statusline` adapter also needs an account. Missing from the sheet.
  - "`resolve` ... Walk `ancestors` inside `spawn_blocking`." The sheet does
    not say how an ancestor pid is matched against the board. By pid alone,
    or by `ProcKey` (pid plus start, via `proc::key_of`)? The stub imports
    `ancestors` and not `key_of`, so I matched by pid, but `ProcKey` exists
    precisely because pid alone is not identity. Both pass the test. This is
    a design decision the sheet should make or turn into a kata.
  - The doc comment on `resolve` says "the nearest ancestor of the
    forwarder's parent", `HookFrame.pid` says "the forwarder's parent pid",
    and the test passes pid 70 whose comm is `switchboard-hoo`, i.e. the
    forwarder itself, and expects 50. All three agree only if the walk is
    inclusive of the pid given. It is (`ancestors` starts at `pid`), but the
    three descriptions do not say the same thing.
  - "`Subscribe` writes a snapshot now, then one more on every
    `view.changed()`, until the peer or the view goes away." True, but "the
    peer goes away" is only detected when a write fails, which is the next
    view change. A subscriber that disconnects during a quiet period leaves
    a task parked on `changed()` until the next change. Not a bug, but the
    sheet should state that lines after `subscribe` are ignored and that
    detecting peer close promptly would need a `select!` against
    `next_line()`. It is a natural stretch item that is not listed.
  - The test comment on `should_ignore_bytes_beyond_the_connection_cap`
    says "The handler stops reading at the cap and drops its read half".
    My handler does not drop anything explicitly: `Take` reports EOF at the
    cap, `lines` yields the truncated line, `decode_request` fails, the
    error reply goes out, `next_line` returns `None`, the function returns
    and both halves drop. The test accepts either. The comment describes
    one implementation, not the requirement.
  - "`daemon::bind` is provided. Read how it refuses to steal a live daemon's
    socket." Accurate and worth reading; the end-to-end test asserts both
    behaviours.
  - Test names all match what they check.
- **Where I got stuck:** not on the real attempt. Two probes of what a
  first-timer would hit:
  - Probe A: `write_line(&mut write, &snapshot_line(&view.borrow())).await`
    in the `List` arm. The `watch::Ref` temporary lives to the end of the
    statement, across the `.await`. The error is "future cannot be sent
    between threads safely ... `RwLockReadGuard` ... used across an await",
    and it is reported at the `tokio::spawn(handle(..))` call in `serve`,
    not at the borrow in `handle`. If the learner writes `handle` before
    `serve` and runs `cargo check`, it passes; the error only appears when
    they write `serve` or run the tests (whose `connect` spawns `handle`).
    The sheet warns about this for `resolve` only. The same rule bites in
    `handle` twice (`List`, `Subscribe`), and the error surfaces in another
    function.
  - Probe B: `spawn_blocking(|| ancestors(proc_root, pid, ..)..)` without
    `move` or an `Arc` clone gives three errors at once: E0373 twice
    (`tracked`, `pid` borrowed, suggests `move`) and E0521 (`proc_root`
    escapes the function body, requires `'static`). Adding `move` as rustc
    suggests fixes the first two and leaves E0521, because `move` captures
    the `&Arc<Path>` reference, not the `Arc`. Only then does the learner
    reach for `Arc::clone`. This two-step is good pedagogy and the sheet
    already says "it needs an owned `Arc` clone and an owned `Vec`", so a
    learner who reads carefully skips it. The doc comment on `resolve`
    says the same. Well signposted.
- **What this drilled:** trait-bounded generics (`R: AsyncRead + Unpin`),
  the `'static` requirement of `spawn` and `spawn_blocking` and the
  `move` plus `Arc::clone` idiom, `!Send` guards across `.await`,
  `try_send` as a backpressure policy, and the shape of a line-protocol
  server (decode, match, reply, keep serving after an error). Incidental
  complexity: the frame protocol and the two adapters are provided and
  their signatures have to be looked up; `FakeProc` and `board_with` in the
  test helpers are opaque until read (the `start: 1000` agreement between
  `key()` and `FakeProc::add(.., 1000, ..)` is what makes the resolve test
  pass and is not commented). The `Subscribe` arm's semantics (stop reading
  the peer) are a design call, not a Rust lesson.
- **Ordering:** T11 after T10 is right: `Subscribe` is the consumer side
  of T10's `watch`, and `offer` is the producer side of T10's inbox, so the
  learner sees both ends of both channels within two tasks. `resolve` is
  separable: it depends only on `proc::ancestors` (T06) and a
  `watch::Receiver`, and it is the densest Rust lesson in the task. It
  could be the first bullet of the sheet ("do this first, it is the
  hardest ownership lesson and it does not need the socket") rather than
  the fourth. The end-to-end test reaching back to `client::send` from T02
  is a satisfying closure and worth calling out on the sheet.
- **Confidence after:** solid on `into_split`, `try_send`, `spawn_blocking`
  and the `'static` closure idiom. Shaky on: when a future must be `Send`
  (the learner will learn it only if they hit Probe A); `Unpin` (kata 1
  asks about it, but nothing in the task forces a confrontation with `Pin`,
  so the answer will be recited from docs); and what a handler should do
  when the peer disconnects mid-subscribe (never tested).
- **Suggestions:**
  1. Replace "then loop over `lines()`" with: "`BufReader::new(read.take(n))
     .lines()` is a `tokio::io::Lines`, not an iterator and not a `Stream`:
     drive it with `while let Ok(Some(line)) = lines.next_line().await`."
  2. Move the `watch::Ref` warning out of the `resolve` bullet into its own
     paragraph: "Anywhere in this file: never let a `view.borrow()` live
     across an `.await`. Pull what you need into a local first. The
     compiler reports this as 'future cannot be sent between threads' at
     the `tokio::spawn` in `serve`, not at the borrow, so write `serve`
     early or run the tests to see it."
  3. Add to the `Statusline` bullet: "the statusline adapter also takes an
     account; use `default_account()` until a source names one."
  4. Decide the ancestor match. Either "match ancestors by pid against the
     tracked keys' pids" or "match by `ProcKey` using `proc::key_of`, one
     extra `stat` read per ancestor", and say why. Or make it kata 4:
     "`ProcKey` carries a start time so a reused pid is not the same
     process. `resolve` matches by pid. When is that wrong, what does it
     cost to fix, and what limits the damage?"
  5. State the `Subscribe` semantics: further lines from the peer are
     ignored, and a peer that hangs up is noticed on the next write.
     Offer the `select!` on `changed()` and `next_line()` as a stretch.
  6. Reword the doc comment on `resolve` to "the nearest tracked ancestor
     of `pid`, `pid` included", so it agrees with `HookFrame.pid` and the
     test.
  7. In `tests/common/mod.rs`, comment that `key()` and
     `FakeProc::add` agree on `start = 1000` on purpose, or make
     `FakeProc::add` take a `ProcKey` so the agreement is visible at the
     call site.

### T12 — async III: tasks that own things

- **Effort:** 1 real cycle (passed on the first attempt), 1 clippy, 1 full
  suite, plus 1 combined probe that produced 2 errors (both E0382, use after
  move). Zero compiler errors on the real attempt. Difficulty 3/5: four
  files and the widest surface of the three async tasks, but each piece is
  short and the central idea (move state into `spawn_blocking`, get it back
  in the return value) is stated plainly on the sheet. About 65% reading:
  `tail.rs` and `LineCtx` from T04/T05 (to remember my own API), `paths.rs`
  for `transcript_path` and `temp_sibling`, the provided `scanner::run` and
  `history::run` as worked examples, and the test's `sentinel()` helper.
  Same caveat as before on what my cycle count measures.
- **Prerequisites assumed:**
  - `tokio::time::interval` and `MissedTickBehavior::Delay`. The imports
    hint at both and `scanner::run` is a complete worked example of the loop
    shape (interval, `spawn_blocking` with an owned clone, `let .. else`),
    but the sheet does not say "copy the shape of `scanner::run`". It
    should; that one sentence turns the watcher from a design problem into
    a transcription.
  - `mpsc::Sender::is_closed`. The sheet says "Check at the top of the
    loop" without naming the method.
  - That the watcher is where JSON is parsed: the transcript adapter takes
    `&Value`, and the tail yields `String`s, so `serde_json::from_str` sits
    between them and a line that fails to parse is skipped. Nothing says
    this; you find it by reading `line`'s signature.
  - `HashMap::drain` for `abort_all`, and that `insert` and `remove` return
    the previous value (the sheet says so for `insert`).
  - Dropping a `spawn_blocking` `JoinHandle` detaches rather than cancels.
    Only kata 2 touches it, and a learner who has not met it will guess
    wrong.
  - `tokio::fs` versus `std::fs` inside `spawn_blocking` for `write_atomic`.
    The `fs` feature is on, so `tokio::fs` works, but the sheet does not say
    which to use and the provided `history::run` uses `std::fs` inside
    `spawn_blocking`, which points the other way.
  - `temp_sibling` returns `Option`, so `write_atomic` needs an
    `ok_or_else(|| io::Error::new(..))`. A small combinator lesson the sheet
    does not mention.
  - The `sentinel()` test helper: a task whose guard sets a flag on drop, so
    "aborted" is observed as "dropped". Clever and worth a comment in the
    test explaining that `abort` is observable only as the future being
    dropped.
- **Sheet accuracy:**
  - "moves it into `spawn_blocking(move || poll_once(taken, &path,
    replay))`". The snippet elides where `path` comes from. Written
    literally as `&spec.path` inside the `move` closure, it moves
    `spec.path` into the closure and fails with E0382 "value moved into
    closure here, in previous iteration of loop" (probe D). The learner
    needs a per-tick `clone()` of the `PathBuf` or an `Arc<Path>` held
    outside the loop. This is the one real ownership lesson in the watcher
    and the sheet skips over it.
  - "The first successful read replays the tail at `Confidence::Inferred`.
    Everything after that is `Confirmed`." Ambiguous on whether a
    successful read with zero lines counts. I said yes (an empty file at
    discovery has nothing to replay). The tests do not distinguish.
  - "`run` maps `WatchTranscript`, `Forget` and `AppendHistory`". The match
    must also cover `PublishState`, which never arrives (the core consumes
    it). Say so, or the learner wonders whether their T10 `dispatch` is
    wrong.
  - The `WatchSpec` needs an `account`; the sheet does not say what to put
    there. `default_account` is imported, which is the hint.
  - "`write_atomic`. Write to `temp_sibling`, then `rename`." The test
    writes to `nested/state.json` inside a fresh scratch dir, so the parent
    directory must be created first. Not on the sheet.
  - "Stop when the inbox is closed. Check at the top of the loop: a quiet
    transcript never sends, so a failed send would never tell it." True and
    well reasoned. But the only test for it drops the receiver *before* the
    task starts, so a watcher that checks `is_closed` only once, or one
    that only ever notices a failed send, passes as long as the first tick
    sends something. The test does not pin the behaviour the sentence
    describes.
  - "`sweep_inputs` ... Return the snapshot first, then one `Died` per key
    in `known.difference(&seen)`. The ordering is load-bearing; the module
    comment says why." Accurate, and the module comment does say why.
    Missing: "collect `seen` before moving `rows` into the snapshot", which
    is the borrow-before-move ordering probe C hit (E0382 "borrow of moved
    value: `rows`").
  - "Done when `t12_tasks` passes, and `cargo test` is green across the
    whole crate." Cannot be true in this copy because the three
    learner-written test files are stubs, but that is the starting state,
    not the sheet.
  - The `Drop` guard comment in `effects.rs` ("Keep or remove the guard once
    `abort_all` is implemented") is accurate and I removed it: `abort` does
    not panic.
  - Test names match behaviour throughout.
- **Where I got stuck:** nowhere on the real attempt. Probes C and D above
  are the two errors I would expect a first-timer to see, and both are
  E0382 with a clear compiler message, so they cost one cycle each, not
  more.
- **What this drilled:** ownership handed into a blocking closure and
  handed back through its return value (the task's thesis, and it lands),
  `JoinHandle::abort` as the lifecycle primitive with `Drop` as the
  backstop, `HashMap` insert/remove returning the old value, `BTreeSet`
  difference, and two ordering lessons (borrow before move; clone before
  move into a loop closure). Incidental complexity: remembering the T04
  `Tail` API (`tail_start` then `Tail::starting_at`), the T05 `LineCtx`
  shape, the JSON parse step nobody mentions, and the `interval` API. None
  of those are Rust lessons; they are "read your own earlier code"
  lessons, which is fine for a course but should be budgeted.
- **Ordering:** T12 should be the last of the three async tasks. It reaches
  back to T04, T05, T06 and T09 and forward-references nothing, and its
  `Watchers` and `write_atomic` pieces are the smallest self-contained
  units in the whole range. Inside the task, the sheet's order (watcher,
  effects, scanner, publish) is hardest-first. Reversing it (`write_atomic`
  in five minutes, `sweep_inputs` in ten, `Watchers` in ten, then the
  watcher) gives a learner three green tests before the one piece that
  needs thought. The watcher itself could also be split: `open_at_tail`
  and `poll_once` are synchronous and testable with a real file, and
  `run` is the async shell around them.
- **Confidence after:** solid on the by-value round trip through
  `spawn_blocking`, and on `abort` plus `Drop` as the cleanup pair. Shaky
  on what `abort` does to blocking work already running (only kata 2 asks),
  on whether `is_closed` is really needed (the test does not prove it), and
  on file replacement: if the transcript is deleted and recreated, the
  open `File` follows the old inode, its length never grows, and the
  watcher goes quiet forever. Never mentioned anywhere.
- **Suggestions:**
  1. Open the sheet with "`scanner::run` and `history::run` are provided
     and are the two shapes you need: a ticking loop that moves an owned
     clone into `spawn_blocking`, and a receive loop that does the same.
     Read both before writing the watcher."
  2. Under the watcher bullet: "The closure needs an owned path. Clone
     `spec.path` each tick, or hold it as an `Arc<Path>` outside the loop.
     `&spec.path` inside `move ||` moves the field and fails on the second
     iteration."
  3. Say who parses: "`Tail::poll` yields `String`s and `claude::line`
     takes a `&Value`. Parse each line with `serde_json::from_str`; a line
     that does not parse is skipped, not fatal."
  4. Name `Sender::is_closed` in the "check at the top of the loop" bullet,
     and add a test that drops the receiver while the watcher is following
     a quiet file, so the guard is actually pinned.
  5. `write_atomic`: "create the parent directory first; map
     `temp_sibling`'s `None` to an `io::Error`; `tokio::fs` is fine here,
     or use `spawn_blocking` as `history::run` does. Say which you chose."
  6. `sweep_inputs`: "collect `seen` from `rows` before `rows` moves into
     the snapshot."
  7. Reorder the sheet's Implement list: `publish.rs`, `scanner.rs`,
     `effects.rs`, then `watcher.rs`.
  8. Add a kata or stretch on file replacement: "The transcript is deleted
     and recreated. What does `poll_once` see? What would you compare to
     notice, and where would you do it?"
  9. Add a one-line comment to the test's `sentinel()` saying that abort is
     observable only as the future being dropped, which is what the flag
     records.

### T13 — Capstone: run it on your own agents

- **Effort:** 1 clippy after deleting `#![allow(unused)]` (zero warnings,
  zero errors), 1 release build (7.5 s), 2 daemon runs of 40 s and 45 s each
  under `timeout`, plus a handful of socket probes with a 30-line Python
  client. Difficulty 1/5 for the steps I could do; the step I could not do
  (5, hooks) is where the capstone's substance lives, see below. Time split:
  almost none on Rust, most on checking that what the daemon reported
  matched the machine (session files, `procStart`, transcript tails).
- **Prerequisites assumed:**
  - Step 1 assumes `#![allow(unused)]` is hiding something. Once every
    stub is filled it hid nothing: clippy was clean the moment I deleted
    it. `model.rs` carries its own `#![allow(dead_code)]`, which still
    hides whatever is unused there (section 10 Federation, `PaneKey`,
    `Fleet`, and friends the header lists as "no task touches"). The sheet
    should say whether that one stays, because the step as written implies
    a cleanup that does not exist.
  - Step 4 assumes `socat`. Not installed here. The sheet anticipates a
    `python3` fallback but does not give it, and "one line, then read
    forever" over `AF_UNIX` with a retry-on-connect is about thirty lines
    once you handle the subscribe stream. Give the snippet.
  - Where output lands. The sheet says to watch the socket, but the more
    convincing artefacts are `state.json` and the history file, and it
    never says where they are (`$XDG_CACHE_HOME/switchboard-rust/state.json`
    and `$XDG_STATE_HOME/switchboard-rust/history/<day>.jsonl`, both
    defaulting under `$HOME`). I found them by reading
    `Layout::from_env`.
  - Two other dry-run copies share this machine. The sheet's socket name
    `$XDG_RUNTIME_DIR/sbr.sock` would collide between them (`bind` refuses
    to steal a live socket, so the second learner would get "a daemon is
    already listening"), and all three would write the same `state.json`
    under `~/.cache`. I used `sbr-dryrun-c.sock` and pointed
    `XDG_CACHE_HOME` and `XDG_STATE_HOME` at my scratch directory. A solo
    learner will not hit this, but the instruction "on its own socket so it
    can't collide with the Go daemon" should extend to the state and
    history paths for the same reason.
- **Sheet accuracy:**
  - "Within a second it should discover every running `claude` and replay
    each transcript tail." True. The first `list` after two seconds showed
    three agents with names and session ids read from
    `~/.claude/sessions/<pid>.json` (I checked `procStart` against field 22
    of `/proc/<pid>/stat` by hand: all three matched). The history file
    shows the replay: three `session_start` rows at the same millisecond,
    then `shown` rows `unknown -> working -> idle` per agent as the
    transcript tails were folded. Thirty history rows over two runs.
  - `state.json` was written and no `state.json.tmp` was left behind.
    `list` and `subscribe` both worked; `subscribe` sent one snapshot and
    then nothing for twelve seconds because nothing changed, which is the
    R6 behaviour. A bad line got `{"ok":false,"error":"malformed JSON:
    ..."}` and the connection stayed up. The socket was mode 0600.
    SIGTERM from `timeout` produced a clean exit and removed the socket
    both times (`timeout` reports 124 regardless of the child's status,
    which will confuse a learner who expects 0; worth a sentence).
  - "Then watch the chip go from green to red and back." There is no chip.
    The Rust daemon writes its own `state.json` and serves its own socket;
    nothing on the machine renders either. What the learner can watch is
    the `phase` field in the subscribe stream or `lamp` in `state.json`.
    The sheet should say that, or ship a ten-line watcher that prints
    `name phase reason` per change, because "see it work" currently means
    "read JSON".
  - One observation about truth versus reality: the lead session on this
    machine (the one orchestrating three teammates) showed `idle` after
    replay, because its last transcript lines were assistant text with no
    open tool call. It is in fact mid-turn, waiting on teammates. That is
    exactly the "transcript alone cannot see a turn that is waiting on
    something external" gap the hooks exist to close, and it is a nice
    thing for the sheet to point at: "compare `truth` with what you know
    the agent is doing, before hooks and after".
  - Step 5, skipped, and what I would do. The instructions for this
    dry-run forbid reconfiguring any Claude session's hooks or starting a
    new `claude`, and the machine's Go daemon already owns the real hooks.
    What I would do: write a settings file
    `~/.cache/sbr-hooks.json` whose `hooks` map registers, for each of
    `UserPromptSubmit`, `PreToolUse`, `PermissionRequest`, `PostToolUse`,
    `PostToolUseFailure`, `Stop`, `StopFailure`, `SubagentStart` and
    `SubagentStop`, one `command` hook whose command is the absolute path
    `<worktree>/target/release/switchboard-hook <Event>`; then start
    `SWITCHBOARD_RUST_SOCKET=$XDG_RUNTIME_DIR/sbr-dryrun-c.sock claude
    --settings ~/.cache/sbr-hooks.json` in a scratch directory, with the
    daemon running in another shell and the subscribe client in a third.
    Expected: on the first prompt a `UserPromptSubmit` frame arrives with
    the forwarder's parent pid, `resolve` walks `claude -> sh ->
    switchboard-hook` to the tracked ancestor, and the agent's `phase`
    goes `idle -> working`; asking it to run a `Bash` command produces
    `PreToolUse` then `PermissionRequest`, and `phase` goes to `blocked`
    with `lamp: red` until I approve, then back to `working`; Escape
    during a turn produces `Stop` with a stop reason that becomes
    `Reason::Interrupted` and an amber lamp; the end of the turn produces
    `Stop` and `idle`. Each of those would also be a `shown` row in the
    history file. Two things I would check that the sheet does not mention:
    that the hook binary is reached by absolute path (the Go daemon's hooks
    use `switchboard-ctl`, so there is no name clash, but `switchboard-hook`
    is not on `PATH`), and that `SWITCHBOARD_RUST_SOCKET` is inherited by
    hook processes (Claude spawns hooks with the session's environment, so
    setting it on the `claude` command line is enough; the sheet says "set
    it in that session's environment", which is correct but could be shown
    as the one-liner above).
- **Where I got stuck:** on my own client, not the daemon: I printed a
  `session` key and the view emits `session_id`, so my first listing showed
  `None` for every session and I spent five minutes checking session files
  and `procStart` before reading `View::to_json`. A learner who knows their
  own `to_json` will not do this; a learner who wrote it nine tasks ago
  might. A printed field list on the sheet would have saved it.
- **What this drilled:** nothing new in Rust. It drilled reading the
  daemon's outputs against the machine: session files, `/proc` stat
  fields, transcript tails, the history log. That is the right capstone
  content, and it is the first time the learner sees their T04 history
  writer, T05 transcript adapter, T06 sweep and T09 settle all produce
  visible artefacts at once. Incidental complexity: the missing `socat`,
  the unstated output paths, the shared-machine collisions.
- **Ordering:** T13 must be last and is. Inside it, steps 1 to 4 are ten
  minutes and steps 5 is the capstone; the sheet's "Core: 30m" is only
  true if step 5 goes smoothly, and step 5 is the least specified step in
  the whole course. Two things would move it forward: give the settings
  file verbatim, and give the subscribe watcher verbatim, so the learner's
  30 minutes go to watching transitions rather than to writing tooling.
- **Confidence after:** high on "it runs and discovers real agents". A
  learner who completes step 5 will have seen every layer fire end to
  end. A learner who cannot (no spare Claude session, or a machine like
  this one where the hooks are spoken for) has seen the transcript path
  only, and will not know whether `resolve`, `offer` or the hook adapter
  work outside the tests. The sheet could offer a fallback: replay a
  captured hook sequence from `docs/product/survey/09-signal-fixtures.md`
  into the socket with the Python client and watch the phases move. That
  exercises T03, T10 and T11 against the release binary without touching
  any real session.
- **Stretch assessment** (not attempted; judged on scope and fit for a
  learner who has just finished T12):
  - *Limit reset expiry.* Well scoped and the right size (a couple of
    hours). It is a design question about `tick` and the core, which is
    T09 and T10 material, and both options the sheet names are workable.
    Best first stretch.
  - *A late replayed `TurnStarted` after `Stop`.* Well scoped, model plus
    reducer, and a good follow-on to T07/T08. It needs step 5 working to
    reproduce, so it is second, not first.
  - *`subagentStatusLine` -> `Snapshot::Subagents`.* A pure adapter in the
    T03 style with a documented payload. This is the easiest item on the
    list and the most self-contained; it is listed fifth and should be
    listed first or second.
  - *`switchboard explain <pid>`.* Well scoped and genuinely useful (a
    ring buffer of inputs per agent, re-folded). It touches the core (T10)
    and the reducer, and a learner who just wrote `run_until_closed` will
    know exactly where the buffer goes. The reference to "open item C1" is
    to the owner's notes that the README told the learner to ignore; drop
    it.
  - *inotify* and *pidfd*. Both are the right direction (each replaces a
    polling loop the learner just wrote) but they are not "a couple of
    hours". They add a crate (`rustix`), and `AsyncFd` is readiness-based
    I/O with `EAGAIN` loops, `clear_ready`, and for inotify an overflow
    protocol, none of which the course has touched. They are a weekend
    each and need a reading pointer (the `AsyncFd` docs and `inotify(7)`
    or `pidfd_open(2)`). They belong at the end of the list, marked as
    such.
  - *Codex hooks.* The README says the course is frozen Claude-only until
    T13 is graded and the Codex adapter is "the material for what comes
    after". Listing it as a T13 stretch contradicts that. Move it to a
    "what comes after" section.
  - Two stretches the journal for T11 and T12 suggest and the sheet does
    not list: prompt detection of a subscriber hanging up (`select!` on
    `changed()` and `next_line()`), and transcript file replacement
    (detect that the open `File` no longer matches the path). Both are
    two-hour items that reuse exactly what T11 and T12 taught.
- **Suggestions:**
  1. Step 1: say what `#![allow(unused)]` is expected to hide at this
     point (probably nothing if every task was completed), and say
     whether `model.rs`'s `#![allow(dead_code)]` stays.
  2. Step 3: list the three output paths and say that `bind` refuses a
     live socket, so a second copy of the daemon on the same socket name
     fails with "already listening" rather than stealing it.
  3. Step 4: give the `python3` subscribe client inline, printing
     `name phase reason` per snapshot, and note that `timeout` exits 124
     on SIGTERM.
  4. Step 5: give the settings JSON verbatim with the absolute hook path,
     and the single command line that sets the socket variable and passes
     `--settings`.
  5. Add a fallback for step 5: replay the captured hook sequence from the
     signal fixtures through the socket with the Python client and watch
     the phases.
  6. Reorder the stretch list: `subagentStatusLine` adapter, limit reset
     expiry, late `TurnStarted`, `explain`, subscriber hang-up detection,
     transcript replacement, then inotify and pidfd marked as weekend
     items with reading pointers, and Codex moved to "what comes after".

## Overall

**Shape of the ramp.** Flat, then a step down. T09 (reducer, snapshots,
time) is the peak of the course for a learner: T10 is a breather (one
function, everything signposted by imports), T11 is a modest step up (more
surface, two ownership lessons whose errors surface far from their cause),
T12 is level with T11 (widest surface but each piece is small and the
central idea is on the sheet), and T13 is ten minutes of running things
plus an unspecified step 5. Measured by my cycle counts it is 2, 1, 1
real compile cycles with zero compiler errors, and the four probes I ran to
find the errors a first-timer would hit found exactly the classes the sheets
already name (a `Send` future in T11, `'static` in T11, use-after-move in
T12) plus one they do not (`Arc<View>` double deref in T10). For a course
whose stated goal is "solidify fundamentals early before harder exercises",
the async range succeeds at the second half: nothing here is harder than
T07-T09. What it does not do is drill. Each task presents each async idea
exactly once, and a learner who copies the shape from the provided
`scanner::run` or `daemon/mod.rs` can finish T10-T12 in an afternoon
without ever having been forced to hold `select!`, `'static` or `Send` in
their head. The katas are where the drilling happens, and they are good;
they should carry more weight (see problem 2).

**The three biggest pedagogical problems.**

1. *Cancellation safety is never mentioned.* T10's loop is correct only
   because `mpsc::Receiver::recv` is cancel-safe, and T11's subscribe loop
   is correct only because `watch::Receiver::changed` is. The sheets say
   `select!` is a gap and then never say the one thing that makes
   `select!` dangerous. A learner leaves T10-T12 able to write a `select!`
   loop and unable to say when one loses data.
2. *The errors that teach are reported far from where they are caused, and
   the sheets do not warn.* The `watch::Ref`-across-await error shows up at
   `tokio::spawn` in `serve`, not in `handle`. The `spec.path` move shows
   up on "previous iteration of loop". `E0521` only appears after rustc's
   own `move` suggestion has been applied. These are the best Rust lessons
   in the range, and a learner who does not hit them (because they read the
   sheet carefully) learns nothing from them, while a learner who does hit
   them gets no help from the sheet. Either turn each one into a kata
   ("this does not compile; where is the error reported and why") or put
   the probe in the sheet as a worked wrong answer.
3. *T13's payoff depends on the least specified step.* Steps 1-4 verify
   that the daemon runs; only step 5 verifies that the learner's T03, T10
   and T11 code works against a real harness. It has no settings file, no
   watcher, no fallback, and on a machine where the hooks are already in
   use it cannot be done. The capstone should be the most carefully
   scripted sheet in the course and it is the least.

**Can T10-T12 be done in any order?** Technically yes: each compiles and
tests on its own against T09. Pedagogically no. T10 introduces the two
channels (`mpsc` inbox, `watch` view) from the owner's side; T11 uses both
from the other side (`offer` into the inbox, `subscribe` off the watch);
T12 spawns the tasks that feed the inbox and reads the effects the core
emits. Doing T12 before T10 means writing `Watchers` and the effect runner
for a core that does not exist yet, and doing T11 before T10 means writing
`subscribe` against a `watch` nobody publishes to. The README's "any order"
claim is true of the build graph and false of the ideas. Recommended order:
**T10, T11, T12**, as numbered, with these changes to the ramp:

- Pull `resolve` to the top of the T11 sheet ("hardest lesson first, and it
  needs no socket") and `write_atomic` plus `sweep_inputs` to the top of
  the T12 sheet ("two green tests in fifteen minutes"), so each task opens
  with a small win and closes with its real lesson rather than the reverse.
- Add the cancellation-safety paragraph and kata to T10, and reference it
  from T11's subscribe bullet and T12's watcher bullet.
- Add the four "worked wrong answers" (T10 `Arc<View>`, T11 `Ref` across
  await, T11 `'static` two-step, T12 move in loop) as katas on their sheets.
- Rewrite T13 step 4 and 5 with verbatim tooling and a fixture-replay
  fallback, reorder the stretch list easiest-first, and move Codex to a
  "what comes after" section as the README already promises.

If the range were to gain a task, the right one is a fourth async task
between T12 and T13 that changes something already working: for example,
replace the watcher's ticker with a `select!` between the ticker and a
shutdown `watch`, or make the subscribe arm notice a peer hanging up. That
is the drill the range lacks: a second pass over `select!`, `'static` and
`Send` in code the learner already owns, with the compiler as the only
signpost.
