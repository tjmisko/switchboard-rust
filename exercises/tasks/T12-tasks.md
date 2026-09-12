# T12 — async III: tasks that own things

**Core: 1h** · Gaps: moving owned state into `spawn_blocking` and getting it
back, `JoinHandle::abort`, `Drop` as cleanup, `HashMap<K, JoinHandle<()>>`,
set difference on `BTreeSet` · Test: `cargo test --test t12_tasks`

## Implement

**`src/daemon/watcher.rs`**: `run(spec, inbox)` and `poll_once`.

- The loop holds `state: Option<(File, Tail)>`. Each tick it `take`s the
  state, moves it into `spawn_blocking(move || poll_once(taken, &path, replay))`,
  and gets `(next_state, Option<Polled>)` back. No `Arc<Mutex<File>>` is
  needed, because ownership goes over and comes back.
- The first successful read replays the tail at `Confidence::Inferred`.
  Everything after that is `Confirmed`.
- Stop when the inbox is closed. Check at the top of the loop: a quiet
  transcript never sends, so a failed send would never tell it.

**`src/daemon/effects.rs`**: `run` and `Watchers`.

- `Watchers::start` aborts any watcher already registered for the process.
  `insert` returns the old value, so this is one `if let`.
- `impl Drop for Watchers` aborts everything still running, so a runner that
  exits can't leak tasks.
- `run` maps `WatchTranscript`, `Forget` and `AppendHistory` (with
  `try_send`: history is best-effort).

**`src/daemon/scanner.rs`**: `sweep_inputs(known, rows, at) -> (Vec<Input>, BTreeSet<ProcKey>)`.
Return the snapshot first, then one `Died` per key in `known.difference(&seen)`.
The ordering is load-bearing; the module comment says why.

**`src/daemon/publish.rs`**: `write_atomic`. Write to `temp_sibling`, then
`rename`.

## Katas

1. `poll_once` takes `state: Option<(File, Tail)>` by value and returns it.
   Sketch the `Arc<Mutex<..>>` version and name two things that got worse.
2. `JoinHandle::abort` on a watcher that is currently inside
   `spawn_blocking(..).await`. What happens to the blocking closure, and to
   the `File` it holds?
3. Why is a recycled pid, with the same pid and a different `start`, both a
   death and a birth in one sweep? What would go wrong if `ProcKey` were just
   the pid?

## Done when

`t12_tasks` passes, and `cargo test` is green across the whole crate.
