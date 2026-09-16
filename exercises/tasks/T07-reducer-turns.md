# T07 — Reducer I: routing, sessions, turns, tools

**Core: 2h, as two halves: T07a 45m, T07b 1h15 (about half of each writing
tests)** · Gaps: **ownership in real APIs**: mutating an enum in place
through `&mut`, split borrows of struct fields, returning a `Copy` key
instead of a reference · Test: **`t07_turns`, which you write**

This is the heart of the daemon, and it's where your model stops being a
sketch. It is one task in two halves with one test file: T07a is the
lifecycle (routing, sessions, death, cwd) and gives you a passing harness
before any semantic decision; T07b is turns and tools, where the decisions
are. Do not start T07b until every T07a group is green.

## Read first

`src/reduce/mod.rs` top to bottom. Its module comment carries the three
conclusions of `docs/02` that this code rests on (the turn holds a map;
phase is a fold, not a field; a child raises attention and never lowers it)
and the vocabulary translation from that document (`Root` → `Agent`,
`Child` → `Subagent`, `Writer` → `Actor`, `stale_after` → `TURN_STALE`), so
`docs/02` itself is optional. Then, in `model.rs`, the doc comments on
`Activity`, `Shown::settle` and `Agent::phase`.

Also read `Effects::push`: an `AppendHistory` implies a publish. That decides
which tests can assert `fx.is_empty()` and which can only assert
`!publishes(&fx)`.

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

The file lists about 30 test names, grouped by theme and labelled by half.
Each stub says which builder it needs where it is not `board_with`
(`board_unobserved` gives agents nobody has heard from, and the difference
is whether an `AgentFound` entry or an `Idle` chip already exists). Before
implementing anything, write the body of every test in one group, run them,
and check they fail. Then implement just enough to pass that group, and move
to the next. No T07 test may use `from_subagent`: the first line of
`transition` hands a subagent's event to `subagent_transition`, which is
T08's and stays `todo!()`.

The golden has its own version of each test, which I run when you grade.
Where your test and mine disagree, one of us has found a spec ambiguity, and
that's worth talking through.

## T07a — Routing, sessions, death, cwd

**`src/reduce/mod.rs`**:

- `route(board, process, session) -> Option<ProcKey>`. Returning the *key*
  is deliberate; the doc comment says why. Try writing it as
  `-> Option<&mut Agent>` first, and keep the compiler error in your answers
  file.
- `limits_for(accounts, agent) -> Option<&'a Limits>`. Look at which argument
  carries `'a`.
- `settle(agent, limits, cause, confidence, now) -> Option<HistoryEntry>`

**`src/reduce/event.rs`**:

- `apply_event`, in the order its doc comment gives: routing; `Died`; the
  `/clear` race; adopting a session; `transition`; `last_event`; `settle`.
  Handle `Died` **before** you destructure, because it removes from
  `board.agents`. Then `let Board { agents, accounts } = board;` so you can
  hold `&mut Agent` and `&Account` at the same time.
- `transition` arms: `SessionStarted`, `SessionEnded`, `CompactionStarted` /
  `CompactionEnded`, `CwdChanged`, and `Died` as a no-op (it was handled
  already). Leave the turn and tool arms and the T08 arms as `todo!()`.
- `idle`, because `SessionStarted` needs it for the reset.

The rules the T07a tests pin:

- Nothing is created for an event naming no tracked agent.
- A pid that's tracked wins over the session. An untracked pid falls back to
  the session.
- **The `/clear` race** is this: `/clear` sends `SessionEnd` then
  `SessionStart{clear}` with a new session id, while hooks and transcript
  lines for the old id are still in flight. So: once an agent has rotated to
  a new session, a late event for the old session is a no-op. And more
  generally, an event from a tracked pid that names *any* session the agent
  does not hold is dropped, not rerouted to whichever agent holds it. Only
  `SessionStarted` may change the session an agent holds.
- An agent with no session yet adopts the first event's, and asks for its
  transcript to be watched.
- `SessionStarted` on an unobserved agent makes it idle. A rotation, or a
  `/clear` start, wipes the conversation: activity, prompts, subagents,
  waits, compaction.
- `SessionEnded` is not liveness: nothing changes, nothing publishes.
- `Died` removes the agent and emits `Forget` and `AgentGone`; a second
  `Died` is a no-op.
- `CwdChanged` renames the workspace; the same path twice is a no-op.
- `settle` runs on every routed event, no-op or not. A duplicate is still
  silent because `Shown::settle` returns nothing when the truth already
  matches the screen. The reason to run it anyway: an escalation whose dwell
  has elapsed promotes on the next event instead of waiting for a tick.

**Checkpoint:** the three T07a groups pass:
`cargo test --test t07_turns -- should_create should_route should_drop should_adopt should_ignore_a_late should_reset should_show_unknown should_go_idle_on should_remove should_follow should_publish_nothing_for_session`.

## T07b — Turns and tools

**`src/reduce/event.rs`**:

- `transition` arms: `TurnStarted`, `TurnEnded`, `ToolDispatched` /
  `ToolSettled` (**agent actor only**; T08 does subagents).
- Helpers: `working`, `end_turn`, `dispatch`, `settle_call`. Their doc
  comments are the rules; `end_turn`'s lists the idle and unobserved cases.
- `end_turn`, `dispatch` and `settle_call` call T08's `*_unjoined` helpers.
  The scaffold ships those as placeholders (`clear_unjoined` and
  `lift_sole_unjoined` return `false`, `take_unjoined` returns `None`, each
  marked `// T08`), so you write the calls now and T08 fills them in. Put
  the calls where the doc comments say, on every path: `end_turn` lifts the
  agent's parked prompts before it looks at the key, idle included, and
  `TurnStarted` lifts them after its duplicate check. T08's
  `should_lift_every_agent_prompt_when_the_turn_ends_or_a_new_one_starts`
  is the test that finds a call in the wrong place.

The rules the T07b tests pin:

- `TurnStarted` with the same key twice publishes nothing, and the duplicate
  check comes before anything else in the arm. A different key mid-turn
  keeps the open calls.
- `TurnEnded` whose key names a different turn is ignored. With no key, it
  ends whatever turn is open.
- `TurnEnded` while **idle**: a non-`Completed` outcome replaces
  `after: None | Completed`; anything else is a no-op. So an interrupt
  landing after `Stop` still records `Stopped`, and a `Completed` landing
  after an interrupt never erases it.
- `TurnEnded` while **unobserved**: goes idle with that outcome, whatever
  it is. A transcript replay's first line can do this.
- `ToolDispatched` while idle starts a keyless turn. A duplicate dispatch
  publishes nothing.
- `ToolSettled{Failed{interrupt: true}}` ends the turn as interrupted. A
  plain failure, or a denial, removes the call and keeps the turn.
- A settle for a call never dispatched publishes nothing (until T08 gives
  it a parked prompt to lift).
- `last_event` is stamped only when something changed.
- De-escalation and user-caused escalation show immediately. An inferred,
  agent-caused escalation is truth-but-not-shown, with `pending` set.

**`src/model.rs`**, last: implement the `unowned_gate_anywhere` stub
(`todo!("T07")`), then make it the body of `Agent::has_unowned_gate` and
delete the stub. Your sketch only checks parked prompts. It must also see
tool gates and subagent gates, and its call in `Agent::phase` must move
above the `Turn → Working` check.
`should_render_an_unowned_gate_grey_never_red_even_mid_turn` fails until you
do both; its stub says how to build the state without T08's `ask`.

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
4. In `dispatch`, bind `tools` from `&mut agent.activity` *first*, then call
   `take_unjoined(&mut agent.prompts, ..)` while `tools` is still live. It
   compiles. Why is borrowing two fields of one `&mut Agent` at once allowed?
   Now replace the field access with a method, `agent.prompts_mut()`, and
   explain the error.

## Done when

Your `t07_turns` passes, both halves. Grading runs the hidden
`t07_turns_golden`, which has 33 tests.
