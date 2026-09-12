# T08 — Reducer II: gates, the join, subagents

**Core: 1h15** · Gaps: `position` + `Vec::remove` to take ownership out of a
collection, `retain`, `find` on `values_mut`, `entry().or_insert_with`,
`Option::take` · Test: `cargo test --test t08_join`

Claude's `PermissionRequest` carries no `tool_use_id`, so a prompt has to be
joined to its call on `(actor, tool_name, input_hash)`, from whichever side
arrives second. Read the doc comment on `Prompts` in `model.rs`, then the
cost model's **R4 — liberal in what lifts**.

## Implement (`src/reduce/event.rs`)

- `ask`: three paths. The call id is known (gate it, and create the call if
  this is a question tool). The call is open and matches (gate it). Otherwise,
  park the prompt.
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
- **`src/model.rs`**: `Agent::has_unowned_gate`, which must now see tool
  gates and subagent gates as well as parked prompts.

## Katas

1. `take_unjoined` uses `position` then `remove(i)`. Why not
   `iter().find(..)` and then remove the found element? What does `find` give
   you back, and why can't you move out of it?
2. In `ask`, `tools.entry(call.clone()).or_insert_with(|| Tool { .. })` returns
   `&mut Tool`. Why does the closure form matter here, as opposed to
   `or_insert(Tool { .. })`?
3. An agent's own `Stop` must not clear a subagent's gate. Find the one line
   in `end_turn` that guarantees it.

## Done when

`t08_join` passes, and `t07_turns` still passes.
