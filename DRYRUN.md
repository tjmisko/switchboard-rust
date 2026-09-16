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
