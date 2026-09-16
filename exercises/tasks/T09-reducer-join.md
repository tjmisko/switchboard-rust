# T09 — Reducer II: gates, the join, subagents

**Core: 1h15** · Gaps: `position` + `Vec::remove` to take ownership out of a
collection, `retain`, `find` on `values_mut`, `entry().or_insert_with`,
`Option::take` · Test: `cargo test --test t09_join`

Claude's `PermissionRequest` carries no `tool_use_id`, so a prompt has to be
joined to its call on `(actor, tool_name, input_hash)`, from whichever side
arrives second. Read the doc comment on `Prompts` in `model.rs`, then the
cost model's **R4 — liberal in what lifts**. This task is `src/reduce/event.rs`
only; the tests are provided and every clause below maps to one.

## Implement (`src/reduce/event.rs`)

Two functions take arms, and which arm goes where is decided by the
*actor*, not the kind:

- **`transition`** (the agent's own events, `Actor::Agent`): its first line
  hands a subagent's event to `subagent_transition`; then the
  `PermissionAsked` and `PermissionSettled` arms, and `SubagentStarted` and
  `SubagentEnded`. Those last two come from the *agent's* hooks (`hook.rs`
  sends them with `Actor::Agent`), so they never reach
  `subagent_transition`; a learner who puts them there finds they never
  arrive. When `subagents.source == Panel`, events may refine rows but never
  add or remove them; under `EventsOnly`, `Started` adds the row and `Ended`
  removes it, plus any prompt that child left parked (the arm comment and
  the `Subagents` doc state the lifecycle).
- **`subagent_transition`** (`Actor::Subagent`): refresh the row's
  `last_seen`; `TurnEnded{Failed}` marks the row `Failed` and clears its
  gate (a child's `StopFailure`, 47 of 56 captured, must never end the
  agent's turn); `ToolDispatched` joins a parked prompt onto the row;
  `ToolSettled` and `PermissionSettled` lift the gate; `PermissionAsked`
  goes to `ask_subagent`. Everything else a child sends is a no-op.

The helpers:

- `ask`: three paths. The call id is known (gate it, and create the call if
  this is a question tool). A *running* call matches (gate it). A call
  already *blocked* on the same `(tool, hash)` matches: that is the same
  prompt said twice (Claude sends `PreToolUse` with the id, then
  `PermissionRequest` without), so it is a no-op. Otherwise, park the
  prompt.
- `ask_subagent`: the row is gated, or the prompt is parked under that
  subagent.
- `park`: a duplicate park, on `(actor, tool, hash)`, is a no-op.
- `take_unjoined(prompts, actor, tool, hash) -> Option<Unjoined>`: the
  oldest matching prompt, **removed and returned by value**.
- `clear_unjoined`, and `lift_sole_unjoined`, which lifts only if exactly one
  prompt for that actor is parked. Try to write the "exactly one" check without
  `.count()`.
- `lift_subagent`: `Option::take` on the row's gate.
- `answer`: with a call id, `Allowed` releases the gate and `Denied` /
  `Abandoned` drop the call. With none (Claude's `PermissionDenied` may
  carry none), it is `lift_sole_unjoined` for the agent, and nothing else:
  a gated call is never released by a call-less answer.
  `should_lift_only_the_sole_parked_prompt_when_a_call_less_permission_settles`
  pins all three cases.
- `blocked` and `hash_admits`, the two small ones.

Replace the T08 placeholders with these bodies and nothing else moves: the
calls to `clear_unjoined` in `end_turn` (first thing, every path, idle
included) and in `TurnStarted` (after the duplicate check) are already
there from T08. Two tests check that they were placed right:
`should_lift_every_agent_prompt_when_the_turn_ends_or_a_new_one_starts`
parks a prompt on an *idle* agent and then sends `Stop`, and
`should_keep_a_prompt_parked_between_the_hooks_and_the_transcripts_report_of_one_prompt`
sends the same `TurnStarted` twice around a park. If either fails, the
call is on the wrong side of a `return` in T08's code.

## Katas

1. `take_unjoined` uses `position` then `remove(i)`. Why not
   `iter().find(..)` and then remove the found element? What does `find` give
   you back, and why can't you move out of it?
2. In `ask`, `tools.entry(call.clone()).or_insert_with(|| Tool { .. })` returns
   `&mut Tool`. Why does the closure form matter here, as opposed to
   `or_insert(Tool { .. })`?
3. An agent's own `Stop` must not clear a subagent's gate. Find the one line
   in `end_turn` that guarantees it.
4. Two identical `AskUserQuestion` calls are open, each gated by its own
   `PreToolUse`. A third, call-less announcement arrives for the same input.
   Why must it be a no-op rather than a third gate, and why must a *running*
   identical call still take a call-less prompt?
5. In `subagent_transition`'s `ToolDispatched` arm, `row` borrows
   `agent.subagents.rows` mutably while `take_unjoined` borrows
   `agent.prompts` mutably. Why does that compile, and why does the golden
   re-fetch the row inside each arm rather than hold one `row` across the
   whole `match`?

## Done when

`t09_join` passes, and `t08_turns` still passes.
