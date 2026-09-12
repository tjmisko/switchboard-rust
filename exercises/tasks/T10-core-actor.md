# T10 — async I: the core actor

**Core: 1h** · Gaps: `tokio::select!`, futures as values, `mpsc` and `watch`,
`Arc` for shared read-only state, testing with paused time ·
Test: `cargo test --test t10_core`

The concurrency design on one line: **one task owns the board**. Inputs arrive
on an `mpsc`, the view leaves on a `watch`, and effects leave on an unbounded
`mpsc`. The core never does I/O and never awaits anything that can stall. It
waits on exactly two things, the next input and the next deadline.

Read `src/daemon/mod.rs` first. It is provided, and it's the wiring diagram in
code.

## Implement (`src/daemon/core.rs`)

- `run_until_closed(board, inbox, view, effects) -> Board`
  - Each pass, recompute `next_deadline(&board)` and build a `wake` future:
    `sleep_until` the deadline, or `std::future::pending()` if there is none.
  - `select!` on `inbox.recv()` against `wake`. On an input, `reduce::apply`;
    on a wake, `reduce::tick(&mut board, now())`. When `recv` returns `None`,
    break and **return the board**.
  - `dispatch` the effects.
- `dispatch`: `PublishState` goes to `publish`. Everything else goes to the
  runner, and a closed runner is ignored.
- `publish`: render, then `send_if_modified`, so subscribers wake only when
  the view actually changed.

Use `crate::daemon::now()`, never `Instant::now()`. It reads tokio's clock,
which is what makes `start_paused` tests work.

## Katas

1. Why must `wake` be rebuilt on every pass of the loop instead of created
   once above it? What goes wrong if it isn't?
2. `let wake = async move { match deadline { .. } };`. What does `move`
   capture, and why is that cheap here?
3. `watch` versus `broadcast` for the view. What does a slow subscriber
   experience with each, and which failure does `watch` make impossible?

## Done when

`t10_core` passes, including the two tests where the only thing that happens
is time passing.
