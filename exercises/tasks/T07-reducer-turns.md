# T07 — Reducer I: routing, sessions, turns, tools

**Core: 2h (about half of it writing tests)** · Gaps: **ownership in real
APIs**: mutating an enum in place through `&mut`, split borrows of struct
fields, `mem::take` / `mem::replace`, returning a `Copy` key instead of a
reference · Test: **`t07_turns`, which you write**

This is the heart of the daemon, and it's where your model stops being a
sketch. Read `src/reduce/mod.rs` top to bottom, then the doc comments on
`Activity`, `Shown::settle` and `Agent::phase` in `model.rs`.

## Write the tests first: `tests/t07_turns.rs`

`tests/common/mod.rs` gives you a builder harness, so a test is three to six
lines:

```rust
let t = Clock::new();
let mut b = board_with(&[(100, "s1")], t.at(0));
let fx = apply(&mut b, by_user(100, "s1", t.at(5), turn_started("p1")));
assert_eq!(shown(&b, 100), Phase::Working);
assert!(publishes(&fx));
```

The file lists about 30 test names, grouped by theme. Before implementing
anything, write the body of every test in one group, run them, and check they
fail. Then implement just enough to pass that group, and move to the next.
The golden has its own version of each test, which I run when you grade.
Where your test and mine disagree, one of us has found a spec ambiguity, and
that's worth talking through.

## Implement

**`src/reduce/mod.rs`**:

- `route(board, process, session) -> Option<ProcKey>`. Returning the *key*
  is deliberate; the doc comment says why. Try writing it as
  `-> Option<&mut Agent>` first, and keep the compiler error in your answers
  file.
- `limits_for(accounts, agent) -> Option<&'a Limits>`. Look at which argument
  carries `'a`.
- `settle(agent, limits, cause, confidence, now) -> Option<HistoryEntry>`

**`src/reduce/event.rs`**:

- `apply_event`: routing, `Died`, the `/clear` race, adopting a session,
  then `transition` and `settle`. Destructure `let Board { agents, accounts } = board;`
  so you can hold `&mut Agent` and `&Account` at the same time.
- `transition` arms: `SessionStarted`, `SessionEnded`, `TurnStarted`,
  `TurnEnded`, `ToolDispatched` / `ToolSettled` (**agent actor only**; T08
  does subagents), `CompactionStarted` / `Ended`, `CwdChanged`.
- Helpers: `idle`, `working`, `end_turn`, `dispatch`, `settle_call`.
- `end_turn`, `dispatch` and `settle_call` call T08's `*_unjoined` helpers.
  Until T08, replace those `todo!`s with placeholders: `clear_unjoined` and
  `lift_sole_unjoined` return `false`, and `take_unjoined` returns `None`.
  Leave the T08 arms of `transition` as `todo!()`.

## Rules the tests should pin

- Nothing is created for an event naming no tracked agent.
- A pid that's tracked wins over the session. An untracked pid falls back to
  the session.
- Once an agent has rotated to a new session, a late event for the old session
  is a no-op.
- `TurnStarted` with the same key twice publishes nothing. A different key
  mid-turn keeps the open calls.
- `TurnEnded` whose key names a different turn is ignored. With no key, it
  ends whatever turn is open.
- An interrupt landing after `Stop` still records `Stopped`, but a `Completed`
  landing after an interrupt never erases it.
- `ToolDispatched` while idle starts a keyless turn. A duplicate dispatch
  publishes nothing.
- `ToolSettled{Failed{interrupt: true}}` ends the turn as interrupted.
- `last_event` is stamped only when something changed.
- De-escalation and user-caused escalation show immediately. An inferred,
  agent-caused escalation is truth-but-not-shown, with `pending` set.

## Katas

1. Inside `if let Activity::Turn { tools, .. } = &mut agent.activity { .. }`,
   assigning `agent.activity = idle(..)` compiles, but assigning it and then
   reading `tools.len()` does not. Explain both in terms of when the borrow
   held by `tools` ends.
2. `end_turn` clones the outcome into `Idle { after }`, even though
   `apply_event(board, event: Event)` owns the event. What would you have to
   restructure in `apply_event` and `transition` to move `kind` instead of
   borrowing it? Is that clone worth removing?
3. `settle(agent, limits_for(accounts, agent), ..)` passes `agent` mutably and
   then borrows it shared in the next argument, and it compiles. Why? The
   golden binds `let limits = ..` first anyway. Why is that clearer, and what
   about `limits_for`'s signature makes the binding legal?

## Done when

Your `t07_turns` passes. Grading runs the hidden `t07_turns_golden`, which
has 31 tests.
