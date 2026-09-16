# T14 — async IV: the second pass, in code you own

**Core: 1h** · Stretch: 1h file replacement · Gaps: `select!` over two
different kinds of future, cancellation safety as a property you check per
future, `biased;`, a `watch` as a stop signal, two `&mut` borrows inside one
loop, a receiver taken by value · Test: `cargo test --test t14_second_pass`

Nothing new to read. Everything this task changes is code you wrote in T12
and T13, and the tests are provided. T11, T12 and T13 each showed you
`select!`, `'static` and `Send` once, with the shape on the sheet or in a
provided module. This is the pass where you change working code of your own
and the compiler is the only signpost. Two parts, in order; each is one
`select!` and the reasoning around it. No public signature changes, and the
T12 and T13 suites must keep passing unchanged.

## Part A: the subscriber notices a hang-up (`src/daemon/rpc.rs`)

Your `Subscribe` arm writes a snapshot, then awaits `view.changed()`. A peer
that disconnects while the view is quiet leaves that task parked until the
next change, which in a quiet daemon is never: the task, its buffers and its
clone of the inbox sender all stay. `t12_rpc` never sees this because every
test there changes the view or drops its sender.

Change the wait so the arm waits on two futures at once, `view.changed()`
and `lines.next_line()`, and acts on whichever finishes first:

- `changed()` returns `Ok`: a change; go round and write the next snapshot.
- `changed()` returns `Err`: the view's sender is gone; return.
- `next_line()` yields a line: the peer sent something after `subscribe`.
  Ignore it and **go back to waiting, without writing another snapshot**.
  The wait is its own loop inside the write loop, not a branch of it.
  `should_ignore_a_further_line_and_still_deliver_the_next_change_when_subscribed`
  pins that no line comes back for it.
- `next_line()` yields `Ok(None)` or an error: the peer closed its side, or
  the connection cap was reached; return.

Golden puts the wait in a helper that takes `&mut view` and `&mut lines`
and returns a `bool`, "did it change". Do the same or inline it; the shape
is the same either way.

**Cancellation safety, checked per future.** Every pass drops the future
that lost. T11 said to check each one against the cancel-safety line in its
own docs, so: `watch::Receiver::changed` has a "Cancel safety" section that
says a `changed` cancelled by another branch has marked no version seen, so
the change is still there next pass. `tokio::io::Lines::next_line` has one
that says it is cancel safe: the bytes it has read of a partial line stay in
the `Lines`' own buffer and the next call carries on from them. That second
guarantee is why the helper must borrow the handler's `Lines` and never
build a fresh one around the reader.

**Ownership.** `lines` is borrowed mutably by the `while let` condition, and
that borrow ends when `next_line().await` yields the line, so the arm can
borrow it again. If you write the helper, its parameter is `&mut Lines<R>`
with `R: AsyncBufRead + Unpin`: that is the bound `next_line` needs, and it
is better than naming the concrete `Lines<BufReader<Take<R>>>`. `view` is
`&mut` because `changed` takes `&mut self`. The T12 rule still holds:
`view.borrow_and_update()` lives in a statement of its own, never across
the `.await`; the compiler reports a slip at `tokio::spawn`, not here.

## Part B: the watcher stops on a signal (`src/daemon/watcher.rs`)

Today the only way to end a watcher is `JoinHandle::abort`, which drops it
at whatever `.await` it is on. Add

```rust
pub async fn run_until(spec: WatchSpec, inbox: mpsc::Sender<Input>, stop: watch::Receiver<bool>)
```

and move the body of your T13 `run` into it. One change at the top of the
loop: where `run` awaited `ticker.tick()`, `run_until` waits on the ticker
and the stop at once. A stop is the value becoming `true` **or** the sender
being dropped, and either ends the watcher at once, without waiting for the
next tick. The `is_closed` check on the inbox stays where it was.

- For the stop arm use `stop.wait_for(|stopped| *stopped)`, not
  `changed()`. It resolves when the value is `true`, including at once if it
  already is when the watcher starts, and it returns `Err` when the sender
  is gone; so the arm is `return`, whatever the result. `changed()` fires
  only on versions the receiver has not seen, and the initial value counts
  as seen, so a stop raised before the first pass is slept through:
  `should_end_promptly_when_the_stop_was_signalled_before_the_first_pass`
  pins the difference. `changed()` would also hand you a `Ref` to read and
  drop before the next `.await`, which is the T12 rule again.
- Start the `select!` with `biased;` and put the stop arm first. Without
  it, a stop raised while a poll was on the blocking pool races a tick that
  came due meanwhile, and the loser of a fair coin is one more poll.
- What a stop does not interrupt: a poll already on the blocking pool, and
  a `send` into a full inbox. The lines the last completed poll read are all
  sent before the loop reaches the `select!` again, which is what
  `should_deliver_what_the_last_poll_read_before_ending_when_stopped` pins.

**Cancellation safety again.** `Interval::tick` has a "Cancel safety"
section: if another branch completes first, no tick has been consumed, so a
tick that loses to a stop is simply never taken. `wait_for` has one too: a
`wait_for` that loses has marked no value seen that satisfied the closure, so
a stop that loses to a tick is still pending, and with `biased;` it wins
the next pass. Say both in your answers file in your own words.

**`run` becomes a delegation.** Keep its signature. Create a `watch`
channel with `false`, hold the sender in a named local for `run`'s whole
body, and await `run_until` with the receiver. The sender must be held: a
dropped sender is a stop, so `let (_, never) = ..` drops it at the end of
that statement and the watcher ends on its first pass, before its first
poll. Four T13 tests notice, and
`should_keep_following_when_run_holds_its_own_stop` says it in one.

**Ownership.** `stop` is taken by value and bound `mut`: `wait_for` takes
`&mut self` and its future borrows the receiver for as long as it is
polled. The tick future borrows `ticker` the same way, and two `&mut`
borrows of two different locals in one `select!` are fine; two of the same
one are not. Why the receiver is not `&mut watch::Receiver<bool>` is kata 3.

## Katas

1. Each `select!` in this task has one arm that is cancelled when the other
   wins. For each of the four (two per `select!`), say what state the
   cancelled future could have lost if it were not cancel-safe, and what the
   visible symptom would be.
2. `run` holds `_stop` for its whole body. With `let (_, never) =
   watch::channel(false);` instead, what happens, which test in which suite
   is the first to notice, and why does the compiler not warn?
3. Suppose `run_until` took `stop: &mut watch::Receiver<bool>`. What changes
   for `effects::run`, which spawns the watcher with `tokio::spawn`, and what
   does the answer say about who should own a spawned task's inputs?

## Stretch (1h, described, not implemented)

- **A graceful stop in `Watchers`.** Give each entry a
  `watch::Sender<bool>` next to its `JoinHandle`, spawn `run_until` with the
  receiver, and make `stop` and `abort_all` send `true` first and `abort`
  only a task that has not finished after a short grace. The sender lives in
  the map, which is kata 2 in another place; `Drop` still aborts, because
  `drop` cannot await. Test it with the `sentinel` pattern from `t13_tasks`
  and a real `run_until` on a quiet file.
- **Transcript replacement.** T13's kata 4 made real: a transcript deleted
  and recreated at the same path leaves `poll_once` reading the old inode
  forever, and `Polled::truncated` does not cover it. In `poll_once`, before
  `tail.poll`, compare the open file's `MetadataExt::ino()` with a fresh
  `std::fs::metadata(path)` of the path (`use std::os::unix::fs::MetadataExt`)
  and, when they differ, drop the file and reopen at the tail as if the
  state were `None`. Decide what a path that is missing right now means
  (golden would keep the old file and retry). Test it with a rename over
  the watched file, and say which confidence the lines after a reopen carry.

## Done when

`t14_second_pass` passes, all nine, and `t12_rpc` and `t13_tasks` still
pass unchanged.
