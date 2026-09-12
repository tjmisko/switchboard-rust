# Kata answers (golden only — not shipped to `main`)

## T01
1. E0106, missing lifetime: there is no input reference for `&str` to borrow
   from, and `ids` is dropped at the end of the function. The two fixes change
   ownership in opposite directions. Either return owned data
   (`-> String`, `ids[0].to_string()`, or `-> SessionId` via `swap_remove`), or
   borrow the input (`fn first(ids: &[SessionId]) -> &str`).
2. `n: &&str`. `iter()` yields `&&str` and `filter` passes a reference to the
   item. Method calls auto-deref through both layers, so `n.len()` resolves to
   `str::len`.
3. Deref coercion only goes from `&SessionId` to `&str`. Nothing converts a
   `SessionId` into a `ToolUseId`, so the ids stay distinct types, and that
   property survives. What is weakened is encapsulation: every `str` method
   is now callable on an id, so for example `id.len()` compiles.
   The general rule is the "Deref is for smart pointers" guideline. A
   read-only string newtype is a defensible exception to it.

## T02
1. `ok_or` is right. `Missing("cmd")` is a `Copy`-sized literal with no
   allocation, so building it eagerly costs nothing. `ok_or_else` matters when
   building the error allocates or computes something, e.g.
   `UnknownCommand(other.into())`, or a `format!`.
2. `Option<&Value>` → `Option<Result<u32, FrameError>>` →
   `Result<Option<u32>, FrameError>`.
3. std provides `impl<'a, E: Error + Send + Sync + 'a> From<E> for Box<dyn Error + Send + Sync + 'a>`.
   What you wrote are the `Error` impls, plus the `Display` they require. Those
   are what make each type qualify for that blanket impl.

## T03
1. `Option<&Value>`, then `Option<&str>`, then `Option<SubagentId>`, then
   `Actor`. A tuple-variant constructor is a function
   `fn(SubagentId) -> Actor`, so it can be passed wherever a closure is
   expected.
2. E0515: it returns a reference into `v`, which is dropped at the end of the
   function. Either return owned data (`-> Option<String>`, via `.map(str::to_owned)`),
   or take the parsed value (`fn session_of(v: &Value) -> Option<&str>`).
3. With `key: &'a str`, the result would be tied to `key`'s lifetime as well,
   so a temporary key would make the result unusable after the key drops. That
   is exactly what the named test does.

## T04
1. `impl<R: Read + ?Sized> Read for &mut R`. Yes, `file` is usable again once
   the `BufReader<Take<&mut R>>` drops, because the mutable borrow ends there.
   One subtlety: a `BufReader` reads ahead, so the file's position can be past
   what you consumed. `Tail` sidesteps this by tracking `offset` itself and
   seeking on every poll.
2. `lines()` lives on `BufRead`. Finding a `\n` needs a buffer you can inspect
   and then consume from. `File` implements only `Read`. Hand-rolled
   byte-at-a-time reads make one syscall per byte.
3. `new(&mut file)` mutably borrows `file` for the life of that writer, which
   ends each loop iteration. `new(file)` moves `file` into the writer, which is
   a temporary, so the file is closed at the end of that statement.

## T05
1. Keys would borrow from `Value`s created inside the function, which die when
   it returns. To take `&[String]` the return type would have to own its keys
   (`BTreeMap<String, Usage>`).
2. The items are `Input`. `flat_map` accepts any `IntoIterator`, and
   `Vec<Input>` is `IntoIterator<Item = Input>`.
3. `then` takes a closure and evaluates lazily. `then_some` takes a value, so
   `event(..)` runs, and allocates, even when the condition is false.

## T06
1. The closure passed to `successors` captures `proc_root: &Path`, so the
   iterator borrows it. `'_` names that borrow in the return type.
2. Every test except `should_split_cmdline_on_nul_and_borrow_when_valid_utf8`
   would still pass. The cost is one allocation per argument per process per
   sweep, which works out to a few hundred per second on a busy machine.
3. `comm: &str`, by deref of `&String`. `exe: Option<&Path>`, via `as_deref`.
   `argv: &[Cow<'_, str>]`, by deref of `&Vec<_>`. `S = Cow<'_, str>`.

## T07
1. NLL ends the borrow at its last use. If `tools` is not used after the
   assignment, the borrow has already ended and the assignment is legal. If
   `tools.len()` comes after, the borrow is still live across the assignment,
   so you are writing to `agent.activity` while holding a reference into it
   (E0506).
2. `transition` would take `event: Event` by value. You would copy `cause`,
   `confidence` and `at` out first (they're `Copy`), clone `session` for the
   session check, then `match event.kind` by value. It's workable. The clone
   is one small enum, rare, and it keeps `transition(&Event)` simple, so it
   isn't worth removing.
3. Two-phase borrows. The `&mut *agent` reborrow for the first argument is
   *reserved* but not *activated* until the call, so the shared borrow in the
   second argument is allowed. The binding is clearer because it doesn't rely
   on that subtlety. It's legal because `limits_for<'a>(accounts: &'a .., agent: &Agent) -> Option<&'a Limits>`
   ties the result only to `accounts`, so the borrow of `agent` ends when
   `limits_for` returns.

## T08
1. `find` returns `&Unjoined`, a shared borrow of the vector. While that
   borrow is live you can neither `remove` from the vector nor move out of the
   reference. `position` returns a `usize` (`Copy`), which ends the borrow, and
   `remove(i)` then returns the element by value.
2. `or_insert(Tool { name: tool.clone(), .. })` builds, and clones into, a
   `Tool` even when the entry already exists. `or_insert_with` builds it only
   when the entry is vacant.
3. `clear_unjoined(&mut agent.prompts, &Actor::Agent)` removes only prompts
   held by `Actor::Agent`. Subagent gates live on `subagents.rows`, which
   `end_turn` never touches.

## T09
1. `agent` is `&mut Agent`, and you cannot move a field out from behind a
   reference (E0507). `mem::take` swaps in `Default::default()` and hands you
   the owned original.
2. `waits` and `prompts` borrow `agent`'s collections. `pending`, `turn` and
   `compaction` are `Option<Instant>`, already owned because they are `Copy`.
3. `settle` consults `confidence` only when it *creates* a pending escalation.
   A pending escalation to the same phase keeps the `promote_at` it was given
   when the snapshot arrived.

## T10
1. The deadline can move with every input. A future built once keeps the old
   deadline, so it can fire late or not at all. There's a second problem: a
   `Sleep` that has already completed resolves immediately on every later
   poll, which turns the loop into a busy spin.
2. It captures `deadline: Option<std::time::Instant>`, which is `Copy`, so the
   move is a few bytes.
3. With `broadcast`, a slow subscriber gets `Lagged` and loses messages, while
   the buffer holds every intermediate view. With `watch`, it skips straight to
   the latest view. `watch` makes a growing backlog of stale views impossible,
   which suits state as opposed to events.

## T11
1. The generic halves let the tests use `tokio::io::duplex` pipes, with no
   socket and no filesystem. The extension methods (`read`, `write_all`) take
   `&mut Self` and require `Self: Unpin`. Without `Unpin` you would need
   `Pin<&mut W>`.
2. It fails to compile once spawned: `watch::Ref` holds a read guard that is
   `!Send`, so `handle`'s future is not `Send` and `tokio::spawn` rejects it.
   In a context where it did compile, it would block the core's
   `send_if_modified` for as long as the blocking walk runs.
3. Each handler blocks on `send`, so replies stop. After 500 ms each forwarder
   times out, and every hook costs its agent that delay. Connections and tasks
   pile up, and all of those inputs still land late, after their relevance.
   `try_send` drops one input and replies immediately, and the transcript
   backfills most of what was dropped.

## T12
1. Two things get worse. The lock would be held across blocking I/O, and you
   would need `Arc` clones and poison handling, all for state only one task
   ever touches. The exclusive access that the type system proved in the
   moving version becomes a runtime property.
2. Abort drops the task's future at its next `.await`. The blocking closure
   cannot be interrupted and runs to completion. Its result, including the
   `File`, is then dropped with the orphaned `JoinHandle`, which closes the
   file.
3. `ProcKey` includes `start`, so the old key is in `known` but not in `seen`
   (a death), and the new key is in `seen` (a birth). Keyed by pid alone, the
   new process would inherit the dead agent's session, gates and history.
