# Concurrency design: who mutates the session map, and how

Companion to `00-rewrite-plan.md` §3. This records the analysis behind
decision 6 (actor core vs. mutex store) so it does not have to be re-derived.

## 1. The writers

Everything in the Go daemon that independently learns something and writes
it into the session map. Grouped by what wakes it.

**Woken by the kernel or another process**

| Writer | Wakes on | Cadence | Writes |
|---|---|---|---|
| Deathwatch, one per session | pidfd readable | once per session lifetime | remove session, emit `session_end` |
| WM event reader | Hyprland socket2 line | bursty: dozens/s during a re-layout, seconds apart otherwise | `focused` on every session; on close, maybe remove |
| Hook ingest (RPC) | a hook subprocess connects | every tool call, prompt, stop; many/s under fanout | status, prompt sets, session id, transcript path, waits |
| `activity` (RPC) | swayidle | minutes | global idle/active flag |
| Signals | SIGTERM | once | shutdown |

**Woken by a timer**

| Writer | Cadence | Reads first (I/O) | Writes |
|---|---|---|---|
| Scanner | 1 s | `/proc` for every unknown pid | insert session; arm deathwatch |
| Liveness sweep | 5 s | `/proc/<pid>` per session | remove gone sessions; `suspended` |
| Resolver | 5 s, and after a debounced layout event | `wezterm cli list` per mux; Hyprland clients + active window | pane and window per session; capabilities |
| Provider observer | 5 s, plus every hook | transcript tails, `subagents/` dir | agent graph, prompt resolution, children, blocks |
| Deadlines | 30 s grace, 15 s expiry, per-wait caps, reset times | nothing | flip a prompt to red, a graph to grey, a wait to expired, a block to idle |

**Readers only**: `list`, `subscribe`, renderers, `focus`. They need a
consistent snapshot, not the live map.

## 2. The real conflicts

Two writers can disagree about the same field in four places:

1. **Deathwatch vs sweep vs WM close.** All three can conclude "gone" for the
   same pid within a second. Needs one `session_end` writer deduped by map
   membership. Neither architecture removes this; it is a rule, not a
   mechanism.
2. **Resolver vs WM focus events.** The resolver's enumeration is ~100 ms
   old when it lands; a focus event applied in between must not be
   overwritten. Go: the `resolveTurn` mutex held across gather and apply.
3. **Hooks vs provider observer.** A hook lands in microseconds; the
   observer's transcript read takes tens of milliseconds and reasons about a
   prompt set a hook may have changed meanwhile. Go: per-root generation
   counters and a "hook owns transition" flag.
4. **Scanner vs deathwatch** on a pid that dies between `read` and
   `pidfd_open`. Go: ordering on one goroutine.

Everything else (`activity`, capabilities, `suspended`) writes disjoint fields
and never races anything meaningful.

Two observations:

- The timer-driven writers are not independent by nature. Scanner, sweep,
  resolver, and observer are all "read something slow, then update the map".
  Go gives them separate goroutines because that is the cheap way to get a
  ticker. On one loop with a `select!` over three intervals, conflicts 2–4
  mostly stop existing because there is no "meanwhile".
- Rates are low. Hooks under heavy fanout are the worst case at maybe ten
  per second; the measured Go lock hold is 833 ns. Contention is not a
  performance problem in either design. It is a reasoning problem: with nine
  writers on separate tasks you must prove, per pair, that stale-after-fresh
  cannot happen.

So the actor is not there to reduce contention. It is there so that
"meanwhile" cannot happen at all: one place where state changes, events in a
total order.

## 3. Option A: shared store behind a mutex

The Go shape, translated.

```rust
struct Store {
    inner: parking_lot::RwLock<Sessions>,
    publish: watch::Sender<Arc<Snapshot>>,
}
impl Store {
    fn apply(&self, f: impl FnOnce(&mut Sessions)) {
        let mut s = self.inner.write();
        f(&mut s);
        let snap = s.snapshot();          // deep copy
        drop(s);
        self.publish.send_replace(Arc::new(snap));
    }
}
```

Every task holds an `Arc<Store>` and calls `apply` when it learns something.
Consequences:

- No I/O under the lock, by discipline. Nothing enforces it except review
  (agent-watcher used a `!Send` guard so holding the lock across `.await`
  fails to compile; a good trick).
- Stale results need fences: one per conflict above.
- Timers are callbacks with identity checks.
- Easy to port line by line.

## 4. Option B: actor, one task owns the state

```rust
struct Core {
    sessions: HashMap<ProcId, SessionState>,   // owned, no lock
    publish: watch::Sender<Arc<Snapshot>>,
    history: mpsc::Sender<history::Event>,
}
async fn run(mut core: Core, mut inbox: mpsc::Receiver<Event>) {
    while let Some(ev) = inbox.recv().await {
        for effect in core.apply(ev) {     // &mut self, not async: cannot do I/O
            core.perform(effect);          // sends, spawns, deadlines
        }
    }
}
```

Nobody else references `sessions`. Every other task is a producer of
`Event`s or a reader of `Arc<Snapshot>`.

- No I/O under the lock, by type: `apply` is not `async`.
- Ordering is the inbox. The resolver stamps `seq`; `apply` drops an
  `Enumerated` older than the last applied. One comparison replaces
  `resolveTurn`, the generations, and `broadcastGen`.
- Timers are events: `Effect::Deadline{key, purpose, at, generation}` →
  a timer task → `Event::Deadline`; stale generations are ignored. The
  reducer is testable by feeding a `Vec<Event>`, no sleeping.
- `focus` does not go through the core: it reads the `watch` snapshot and
  does its own WM/terminal I/O. "One owner" is not "one task does everything".
- Porting cost is medium: Go goroutines that mutate directly must be reshaped
  into producers, and their logic moves into `apply`. The transition tables in
  survey 03 already are that logic.

## 5. Comparison

| | Mutex store | Actor |
|---|---|---|
| Who can mutate | any task, serialized by lock | one task |
| I/O-under-lock rule | discipline or a guard type | impossible by type |
| Stale-result handling | one fence per producer kind | one `seq` compare |
| Timers | callbacks with identity checks | events with a generation |
| Reads | lock or `watch` | `watch` only |
| Core testability | needs the store and fakes | `apply(&mut state, event)` is pure |
| Latency | a hook applies on its own task | waits behind the inbox (µs) |
| Failure mode | deadlock, lock-order bug | inbox backpressure; choose bounded drop or block |
| Porting cost | low | medium |

A middle option: a mutex store with a single tick loop doing all slow gathers,
plus the three event-driven writers. One fence to write instead of three.
The actor is the same idea taken to its end.

## 6. Recommendation

Actor. Three reasons in order of weight:

1. It is the Rust-shaped answer to the exact problems the Go code paid for.
   Every fence and lock-order rule in survey 01 §3.2 exists because several
   goroutines share one map. Ownership removes the sharing.
2. The core becomes a pure function, which is where all the hard rules live.
   Survey 03's tables map onto `match (state, event)`; the Go tests that pin
   them port as `apply` tests with no runtime.
3. It teaches the thing the project exists to teach. `Arc<Mutex<T>>` is Go
   in Rust; channels plus a single owner is what the borrow checker wants.

Risk: `apply` grows into one giant function. Mitigation: the core dispatches
root-level provider events to `claude::step` / `codex::step` and keeps only
lifecycle, navigation, and publication for itself.

## 7. Task inventory for v1 (about ten long-lived tasks)

| Task | Waits on | Emits |
|---|---|---|
| core | inbox | `watch<Arc<Snapshot>>`, history mpsc, `Invalidate`, deadlines, watch/unwatch |
| scanner | 1 s interval; reads latest snapshot for the known set | `Appeared` |
| deathwatch[n] | `AsyncFd` on a pidfd | `Died` |
| wm_events | Hyprland socket2 lines; reconnect with backoff | `Wm(_)` |
| resolver | 5 s interval or debounced layout notify; runs `wezterm cli list` + Hyprland clients | `Enumerated{seq,…}` |
| observer | `Invalidate` + 5 s; reads transcript tails and `subagents/` | `Observed{root, seq, delta}` |
| timer wheel | `Effect::Deadline` | `Deadline{key, purpose, generation}` |
| rpc | `UnixListener`; per-connection tasks | `Hook`, `Activity` into the inbox; `focus` does its own I/O from the snapshot |
| publisher | `watch.changed()` | `state.json` via `spawn_blocking`, latest wins |
| history sink | mpsc(512) `try_send` | day-file append on a blocking thread |
| signals | SIGTERM/SIGINT | `CancellationToken` |
