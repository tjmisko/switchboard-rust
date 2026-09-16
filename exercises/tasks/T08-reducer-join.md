# T08 — Reducer II: gates, the join, subagents

**Core: 1h15** · Gaps: `position` + `Vec::remove` to take ownership out of a
collection, `retain`, `find` on `values_mut`, `entry().or_insert_with`,
`Option::take` · Test: `cargo test --test t08_join`

Claude's `PermissionRequest` carries no `tool_use_id`, so a prompt has to be
joined to its call on `(actor, tool_name, input_hash)`, from whichever side
arrives second. Read the doc comment on `Prompts` in `model.rs`, then the
cost model's **R4 — liberal in what lifts**.

## Implement (`src/reduce/event.rs`)

- First, in `transition`: an event is applied to the actor it names. A
  subagent's event goes to `subagent_transition` before anything else
  matches on the kind. A child's `StopFailure` (47 of 56 captured carry
  `agent_id`) must never end the agent's turn.
- `subagent_transition`: refresh the row's `last_seen`; a failed turn marks
  the row `Failed` and clears its gate; tool dispatched joins a parked
  prompt onto the row; tool or permission settled lifts the gate;
  a permission asked goes to `ask_subagent`. Everything else is a no-op.
- `ask`: three paths. The call id is known (gate it, and create the call if
  this is a question tool). A *running* call matches (gate it). A call
  already *blocked* on the same `(tool, hash)` matches: that is the same
  prompt said twice (Claude sends `PreToolUse` with the id, then
  `PermissionRequest` without), so it is a no-op. Otherwise, park the
  prompt.
- `ask_subagent`: the row is gated, or the prompt is parked under that
  subagent.
- `park`: a duplicate park is a no-op.
- `take_unjoined(prompts, actor, tool, hash) -> Option<Unjoined>`: the
  oldest matching prompt, **removed and returned by value**.
- `clear_unjoined`, and `lift_sole_unjoined`, which lifts only if exactly one
  prompt for that actor is parked. Try to write the "exactly one" check without
  `.count()`.
- `lift_subagent`: `Option::take` on the row's gate.
- `answer`: `Allowed` releases the gate; `Denied` / `Abandoned` drop the call.
- The `Subagent` arms of `ToolDispatched` / `ToolSettled` / `PermissionAsked`
  / `PermissionSettled`, plus `SubagentStarted` and `SubagentEnded`. When
  `subagents.source == Panel`, events may refine rows but never add or
  remove them.
- **`src/model.rs`**: implement the `unowned_gate_anywhere` stub (`todo!("T08")`), then make it the body of `Agent::has_unowned_gate` and delete the stub. Your sketch only
  checks parked prompts. It must also see tool gates and subagent gates, and
  its call in `Agent::phase` must move above the `Turn → Working` check.
  `should_render_an_unowned_gate_grey_never_red_even_mid_turn` fails until you
  do both.

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

## Done when

`t08_join` passes, and `t07_turns` still passes.
