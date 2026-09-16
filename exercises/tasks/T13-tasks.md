# T13 — async III: tasks that own things

**Core: 1h** · Gaps: moving owned state into `spawn_blocking` and getting it
back, `JoinHandle::abort`, `Drop` as cleanup, `HashMap<K, JoinHandle<()>>`,
set difference on `BTreeSet` · Test: `cargo test --test t13_tasks`

`scanner::run` and `history::run` are provided and are the two shapes you
need: a ticking loop that moves an owned clone into `spawn_blocking` and gets a
value back, and a receive loop that does the same. Read both before writing
anything here. `src/daemon/mod.rs` shows where each of the four pieces below
is spawned and which channel ends it holds.

## Implement, in this order

The order is easiest first: three green tests in fifteen minutes, then the one
piece that needs thought.

**`src/daemon/publish.rs`**: `write_atomic`. Write to `temp_sibling`, then
`rename`. Create the parent directory first: the test writes to
`nested/state.json` inside a fresh directory. `temp_sibling` returns `Option`;
map its `None` to an `io::Error` with `ok_or_else`. Golden uses `tokio::fs`
for all three calls, which puts each on the blocking pool for you; `std::fs`
inside one `spawn_blocking`, the way `history::run` does it, is equally right
and costs one thread hop instead of three. Say which you chose, and why, in
your answers file.

**`src/daemon/scanner.rs`**: `sweep_inputs(known, rows, at) -> (Vec<Input>, BTreeSet<ProcKey>)`.
Return the snapshot first, then one `Died` per key in `known.difference(&seen)`.
The ordering is load-bearing; the module comment says why. Collect `seen`
from `rows` **before** `rows` moves into the snapshot; the other order is
E0382, "borrow of moved value: `rows`".

**`src/daemon/effects.rs`**: `Watchers`, then `run`.

- `Watchers::start` aborts any watcher already registered for the process.
  `insert` returns the old value, so this is one `if let`. `stop` is the same
  shape with `remove`.
- `Watchers::abort_all` drains the map (`HashMap::drain`) and aborts each
  handle, and `impl Drop for Watchers` calls it, so a runner that exits can't
  leak tasks. `JoinHandle::abort` never panics, so the scaffold's
  `panicking()` guard in `drop` can go once `abort_all` is real.
- `run` maps `WatchTranscript` to a spawned `watcher::run` registered with
  `start`; `Forget` to `stop`; `AppendHistory` to `history.try_send`
  (best-effort: a full appender drops the entry rather than letting a slow
  disk back up into the core). The match must also name `PublishState`. It
  never arrives, because your T11 `dispatch` consumed it; an empty arm with a
  comment saying so is right, and is not a sign that T11 is wrong. The
  `WatchSpec` wants an account: `default_account()`. Its path is
  `transcript_path(&layout.claude_projects, &cwd, &session)`, and its poll
  and replay sizes come from `Settings`.

**`src/daemon/watcher.rs`**: `open_at_tail`, `poll_once`, then `run(spec, inbox)`.
The loop is `scanner::run`'s shape.

- `open_at_tail`: `File::open`, then `tail_start(&mut file, replay_bytes)`
  from T04 and `Tail::starting_at`. `poll_once` opens when the state is
  `None`, then `tail.poll(&mut file)`. A file that is not there yet yields
  `(None, None)` and is retried next tick; a read error keeps the cursor and
  yields nothing. Both are synchronous and can be tried against a real file
  before `run` exists.
- The loop holds `state: Option<(File, Tail)>`. Each tick it `take`s the
  state, moves it into `spawn_blocking(move || poll_once(taken, &path, replay))`,
  and gets `(next_state, Option<Polled>)` back. No `Arc<Mutex<File>>` is
  needed, because ownership goes over and comes back.
- The closure needs an owned path. Clone `spec.path` into a local each tick
  before the closure, or hold an `Arc<Path>` outside the loop. `&spec.path`
  written inside `move ||` moves the field into the closure and fails on the
  second pass with E0382, "value moved into closure here, in previous
  iteration of loop".
- Who parses: `Tail::poll` yields `String`s and `claude::line` takes a
  `&Value`. Parse each with `serde_json::from_str`; a line that does not
  parse is skipped, not fatal. `LineCtx` wants the process, session and
  account from the spec, `now()`, and a confidence.
- The first successful poll sends at `Confidence::Inferred`, even a poll
  that found zero lines (an empty file at discovery has nothing to replay).
  Everything after it is `Confirmed`.
- Stop when the inbox is closed. Ask `inbox.is_closed()` at the top of
  every pass: a quiet transcript never sends, so a failed send would never
  tell it. `should_stop_when_the_inbox_closes_while_the_transcript_is_quiet`
  pins exactly this: it drops the receiver after the replay and then writes
  nothing more.

## Katas

1. `poll_once` takes `state: Option<(File, Tail)>` by value and returns it.
   Sketch the `Arc<Mutex<..>>` version and name two things that got worse.
2. `JoinHandle::abort` on a watcher that is currently inside
   `spawn_blocking(..).await`. What happens to the blocking closure, and to
   the `File` it holds?
3. Why is a recycled pid, with the same pid and a different `start`, both a
   death and a birth in one sweep? What would go wrong if `ProcKey` were just
   the pid?
4. The transcript is deleted and recreated at the same path. The open `File`
   still refers to the old inode: what does `poll_once` see on every tick
   from then on, and what does `Polled::truncated` already cover that this
   does not? What would you compare to notice, and where in `poll_once`
   would you do it?

## Done when

`t13_tasks` passes, all ten tests, and `cargo test` is green across the whole
crate.
