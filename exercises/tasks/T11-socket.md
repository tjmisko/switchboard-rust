# T11 — async II: the socket

**Core: 1h** · Gaps: `AsyncRead` / `AsyncWrite` generics, `into_split`,
`AsyncBufReadExt::lines`, `spawn_blocking` and `'static` closures, `Arc<Path>`,
`try_send` versus `send`, `Send` futures · Test: `cargo test --test t11_rpc`

A hook connection sends one line and waits for one verdict. A subscriber sends
one line and then reads forever. Nothing here may stall an agent, which is why
the inbox is offered to with `try_send` rather than waited on.

**One rule for the whole file: never let a `view.borrow()` live across an
`.await`.** A `watch::Ref` holds a read lock and is not `Send`, so a future
holding one across an await point cannot be spawned. Pull what you need into a
local on its own statement (a `Vec<ProcKey>`, a `String`) so the temporary is
dropped at the semicolon. The compiler does not report this where the borrow
is: it says "future cannot be sent between threads safely" and points at the
`tokio::spawn(handle(..))` in `serve`, or at the test's `connect`, which also
spawns `handle`. `cargo check` on `handle` alone passes. Write `serve` early,
or run the tests, to see it. It bites once in `resolve` and twice in `handle`
(the `List` and `Subscribe` arms).

## Implement (`src/daemon/rpc.rs`), in this order

1. **`resolve(proc_root: &Arc<Path>, pid, view) -> Option<ProcKey>`.** Do
   this first: it is the hardest ownership lesson in the task and it needs no
   socket. Its test is `should_resolve_a_hook_pid_to_the_nearest_tracked_ancestor`.
   - Copy the tracked keys out of `view.borrow()` into a `Vec<ProcKey>`
     **before** anything that could await.
   - Walk `ancestors` inside `spawn_blocking`. The closure has to be
     `'static`, so it needs an owned `Arc` clone and that owned `Vec`. Expect
     two rounds with the compiler: rustc first suggests `move`, which fixes
     the borrowed `pid` and `Vec` and leaves E0521 (`proc_root` escapes the
     function body), because `move` captured the `&Arc`, not the `Arc`.
     `Arc::clone(proc_root)` is the second round.
   - `spawn_blocking(..).await` is `Result<Option<ProcKey>, JoinError>`, so
     the tail is `.ok().flatten()`.
   - The walk starts at `pid` itself. The test passes 70 (the forwarder,
     whose parent is `sh`, whose parent is the agent) and expects 50, and
     passes 50 and expects 50. `HookFrame.pid` is the forwarder's parent
     because `switchboard-hook` reports `parent_id()`, and a hook runs as
     `claude -> sh -> switchboard-hook`; the walk still includes the pid it is
     given so a frame that names the agent directly also resolves.
   - **Matching.** Golden matches each ancestor pid against the `pid` field
     of the tracked keys, not by full `ProcKey`. Reason: `ancestors` already
     reads one `stat` per step for the ppid, and `proc::key_of` would read
     the same file again for the start time. The tracked set is at most one
     scan interval old, and a pid reused inside that second belonged to a
     dead agent whose `Died` the next sweep delivers, so a wrong match lasts
     at most one interval and is then corrected. Kata 4 asks when that is
     not good enough.
2. **`write_line`**: the bytes, a `\n`, then `flush`.
3. **`offer`**: `try_send` each input, and log when it fails.
   `mpsc::error::TrySendError` has two variants: `Full` means the core is
   behind, `Closed` means it is gone. Either way the input is dropped. A
   dropped hook is recovered by the transcript; a hook made to wait would
   stall the agent, and nothing recovers that.
4. **`handle<R: AsyncRead + Unpin, W: AsyncWrite + Unpin>(read, write, inbox, view, proc_root)`**
   - Cap the connection: `BufReader::new(read.take(MAX_CONNECTION_BYTES)).lines()`.
     `take` gives a `Take<R>`, which is only `AsyncBufRead` when `R` is, and
     a socket half is not, so the `BufReader` goes around it. The result is a `tokio::io::Lines`,
     not an iterator and not a `Stream`: drive it with
     `while let Ok(Some(line)) = lines.next_line().await`.
   - Each line goes through `decode_request`. The reply depends on what came
     back:
     - Errors get `error_reply`, and the connection stays open.
     - `Hook` and `Statusline` get resolved to a process, run through the
       adapter with a `Ctx { now: now(), process }`, `offer`ed to the inbox,
       then `ok_reply`. The statusline adapter also takes an account; use
       `default_account()` until a source names one.
     - `List` gets a `snapshot_line` (provided) of `view.borrow_and_update()`.
       Build the `String` in the match arm and await the write after the
       match, so the `Ref` is gone before the `.await`.
     - `Subscribe` writes a snapshot now, then one more after every
       `view.changed()`, until a write fails or `changed` returns `Err` (the
       sender is gone). This is the last request on a connection: further
       lines from the peer are ignored, because this arm never reads again.
       A peer that hangs up is noticed on the next write, which is the next
       view change, so a subscriber that disconnects during a quiet period
       leaves its task parked on `changed()` until then. Noticing it
       promptly needs a `select!` between `changed()` and `next_line()`; both
       are cancel-safe (T10), and that is a T13 stretch, not part of this
       task.
5. **`serve`**: accept in a loop, `into_split` the stream, spawn `handle`
   with clones of the inbox, the view and the root. `into_split` returns two
   owned halves, unlike `split`, which borrows the stream: the spawned future
   must be `'static`, so it has to own everything it holds. The `PathBuf`
   becomes an `Arc<Path>` once, before the loop, with `.into()`.

`daemon::bind` is provided. Read how it refuses to steal a live daemon's
socket and clears a dead one's. The last test serves a real Unix socket to the
synchronous `client::send` you wrote in T02: the forwarder's side and the
daemon's side of the protocol meet for the first time here.

## Katas

1. `handle` is generic over its two halves instead of taking a `UnixStream`.
   What does that buy in `t11_rpc`, and what does `Unpin` have to do with
   calling `.read()` / `.write_all()` on a `&mut W`?
2. What happens if you hold `let r = view.borrow();` across
   `spawn_blocking(..).await`? Is it a compile error or a runtime problem, and
   why? Where is it reported?
3. The forwarder waits 500 ms at most. With `inbox.send(input).await` instead
   of `try_send`, describe the chain of events when the core is slow and ten
   agents fire hooks at once.
4. `ProcKey` carries a start time so a reused pid is not the same process.
   `resolve` matches ancestors by pid. When is that wrong, what would it cost
   to fix with `proc::key_of`, and what limits the damage as written?

## Done when

`t11_rpc` passes. That includes a real Unix socket served end to end to the
synchronous `client::send` from T02.
