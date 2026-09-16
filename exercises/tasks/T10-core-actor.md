# T10 — async I: the core actor

**Core: 1h** · Gaps: `tokio::select!` and cancellation safety, futures as
values, `mpsc` and `watch`, `Arc` for shared read-only state, testing with
paused time · Test: `cargo test --test t10_core`

The concurrency design on one line: **one task owns the board**. Inputs arrive
on an `mpsc`, the view leaves on a `watch`, and effects leave on an unbounded
`mpsc`. The core never does I/O and never awaits anything that can stall. It
waits on exactly two things, the next input and the next deadline.

Read `src/daemon/mod.rs` first. It is provided, and it's the wiring diagram in
code. Then read the header of `tests/t10_core.rs`: it explains `start_paused`
and auto-advance, which is why a ten-second dwell costs the test no real time,
and why the two deadline tests below are not mysterious.

## Implement (`src/daemon/core.rs`)

- `run_until_closed(board, inbox, view, effects) -> Board`
  - Each pass, compute `next_deadline(&board)` **into a local**, then build a
    `wake` future that captures it: `sleep_until` the deadline, or
    `std::future::pending()` if there is none. Calling `next_deadline` inside
    the `async` block instead borrows the board across the `select!`, and the
    other arm's `reduce::apply(&mut board, ..)` cannot borrow it mutably.
  - `next_deadline` returns a `std::time::Instant` and `sleep_until` takes
    tokio's. The alias import `Instant as TokioInstant` is the hint:
    `TokioInstant::from_std(at)`.
  - `select!` on `inbox.recv()` against `wake`. On an input, `reduce::apply`;
    on a wake, `reduce::tick(&mut board, now())`. When `recv` returns `None`,
    break and **return the board**.
  - `dispatch` the effects.

  **Cancellation safety.** `select!` polls both futures and drops the one that
  loses. That is only correct because `mpsc::Receiver::recv` is cancel-safe:
  when the deadline wins, the dropped `recv` future has not taken a message,
  and the next pass receives it. A future that is not cancel-safe loses
  something every time the other arm wins (`AsyncReadExt::read_exact` drops
  the bytes it had read so far; `mpsc::Sender::send` with a value already
  moved in drops the value). Read the "Cancellation safety" section of the
  `tokio::select!` docs before writing the loop, and from here to T12 check
  every future you put in a `select!` against the cancel-safety line in its
  own docs.

- `dispatch`: `PublishState` goes to `publish`. Everything else goes to the
  runner, and a closed runner is ignored. `reduce::Effects` already
  guarantees at most one `PublishState`, placed last, so this is a plain
  loop: no dedupe, no reordering.
- `publish`: render, then `send_if_modified`, so subscribers wake only when
  the view actually changed. The closure receives `&mut Arc<View>` and
  returns whether it changed anything. The payload is an `Arc`, so comparing
  it with the freshly rendered `View` takes a double deref; see kata 4.

Use `crate::daemon::now()` for the tick's timestamp, never `Instant::now()`.
It reads tokio's clock, which is what makes `start_paused` tests work. The
tick arm is the only place in this file that needs a clock.

## Katas

1. Why must `wake` be rebuilt on every pass of the loop instead of created
   once above it? What goes wrong if it isn't?
2. `let wake = async move { match deadline { .. } };`. What does `move`
   capture, and why is that cheap here?
3. `watch` versus `broadcast` for the view. What does a slow subscriber
   experience with each, and which failure does `watch` make impossible?
4. Inside `send_if_modified`, `*current == rendered` does not compile
   (`expected Arc<View>, found View`). Write the two correct spellings, and
   say why rustc's suggested `rendered.into()` compiles but is the wrong fix.
5. Which futures in this loop are cancelled when the other arm wins, and why
   is that safe? Name one tokio future that is not cancel-safe and say what a
   `select!` would lose with it.

## Done when

`t10_core` passes, including the two tests that send a few inputs and then let
only time pass (`should_promote_a_dwell_with_no_further_input` and
`should_end_a_stale_turn_on_its_own_deadline`).
