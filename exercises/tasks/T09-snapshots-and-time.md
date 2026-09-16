# T09 — Snapshots and time

**Core: 1h** · Gaps: `mem::take` to move a field out through `&mut`,
`BTreeMap::remove` returning ownership, `flat_map` over several
`Option`/iterator sources, `min` on an iterator of `Instant`s ·
Test: `cargo test --test t09_snapshot_tick`

Events and snapshots merge by opposite rules. An event is applied once. A
snapshot replaces a whole scope, unless an event has spoken since the snapshot
was taken (`AsOf::supersedes`). Time is the third input: graces expire and
dwells promote.

## Implement

**`src/reduce/snapshot.rs`**:

- `apply_snapshot` arms:
  - `Limits`: newer-or-equal only, publish nothing if the value is unchanged,
    and re-settle every agent on the account.
  - `Subagents`: drop the snapshot if it was taken before `last_event`.
  - `Waits`
  - `Processes` (with `discover` / `discovered`), `Beat` and `Placements` are
    provided. Discovery ships early because every test harness from T07 on
    builds boards through it.
- `replace_panel(old: &Subagents, rows, taken) -> Subagents`: keep each
  surviving row's gate.
- `replace_waits(old: BTreeMap<..>, rows, taken)`: note that it takes the old
  map **by value**. `old.remove(&id)` hands back the old `Wait`, so `armed`
  carries over with no clone. The caller gets that map with
  `std::mem::take(&mut agent.waits)`.

**`src/reduce/tick.rs`**:

- `tick` and `expire`. Staleness may end a turn, but never one that is parked
  on a gate. It also forgets a subagent row past `SUBAGENT_STALE` (54 of 168
  captured children never reported a stop), whoever owns the table, unless
  the row holds a gate.
- `stale_turn_deadline` (one `then`) and `stale_subagent_deadline` (another)
- `next_deadline` and `deadlines`: the pending promotion, the stale turn, the
  compaction, every wait, every parked prompt, and every ungated subagent
  row. That's one `chain` of `Option`s and iterators, `flat_map`ped over
  agents, then `min`.

Discovery is provided, and now carries the session file's `name` onto
`Agent::title`: a first sweep sets it, a later sweep with a different name
renames and publishes, and a sweep with no name leaves it alone.

`view.rs` is provided. `reason()` is a nice read after this task: it follows
the same order as `Agent::phase`.

## Katas

1. `agent.waits = replace_waits(std::mem::take(&mut agent.waits), rows, taken)`.
   Why can't you write `replace_waits(agent.waits, ..)`?
2. `deadlines(agent: &Agent) -> impl Iterator<Item = Instant> + '_`. Which
   sources in the chain borrow from `agent`, and which are already owned?
3. `SNAPSHOT_DWELL` is 6 s and the tick passes `Confidence::Inferred`, yet a
   snapshot escalation still promotes after 6 s rather than 10. Why?

## Done when

`t09_snapshot_tick` passes, along with everything before it.
