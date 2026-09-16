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
