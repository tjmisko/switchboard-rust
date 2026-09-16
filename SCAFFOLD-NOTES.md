# Scaffold notes for the T05–T09 fixes

Changes that exist only in the scaffold (`exercises` branch), because golden
has no marker, no placeholder, or no stub there. Each entry gives the file,
the function, what the scaffold has now, and what it should have. Everything
else in the fix-b commits (sheets, skeleton, tests, fixtures, doc comments,
module comments, the model.rs header rows and `Subagents` docs) applies to
the scaffold as the same edit.

## 1. `src/model.rs`: the `has_unowned_gate` rework is T07, not T08

Three comments and one marker. Golden has none of them (its
`has_unowned_gate` is already the full check and `phase` already tests it
before `Turn`).

**`Agent::phase`**, the comment above the `has_unowned_gate()` call. Now:

```rust
        // T08: this check is in the wrong place for a gate held mid-turn.
        if self.has_unowned_gate() {
```

Should be:

```rust
        // T07b: this check is in the wrong place for a gate held mid-turn.
        if self.has_unowned_gate() {
```

**`Agent::has_unowned_gate`**, doc comment. Now:

```rust
    /// A gate whose owner was never established. Codex hooks report a gate
    /// without saying who holds it.
    ///
    /// T08: this only sees parked prompts. A gate on a call, or on a
    /// subagent, is invisible to it.
    fn has_unowned_gate(&self) -> bool {
```

Should be:

```rust
    /// A gate whose owner was never established. Codex hooks report a gate
    /// without saying who holds it.
    ///
    /// T07b: this only sees parked prompts. A gate on a call, or on a
    /// subagent, is invisible to it.
    fn has_unowned_gate(&self) -> bool {
```

**`Agent::unowned_gate_anywhere`**, doc comment and marker. Now:

```rust
    /// T08: the full check. It sees parked prompts, gates on calls, and gates
    /// on subagents. Once it passes, make it the body of `has_unowned_gate`,
    /// delete this stub, and move the check in `phase` above `Turn`.
    ///
    /// A separate stub because `phase` calls `has_unowned_gate` from T07 on,
    /// and a `todo!()` there would fail every T07 test.
    fn unowned_gate_anywhere(&self) -> bool {
        todo!("T08")
    }
```

Should be:

```rust
    /// T07b, last thing: the full check. It sees parked prompts, gates on
    /// calls, and gates on subagents. Once it passes, make it the body of
    /// `has_unowned_gate`, delete this stub, and move the check in `phase`
    /// above `Turn`. `should_render_an_unowned_gate_grey_never_red_even_mid_turn`
    /// in your t07_turns fails until both are done.
    ///
    /// A separate stub because `phase` calls `has_unowned_gate` from T07a
    /// on, and a `todo!()` there would fail every T07a test.
    fn unowned_gate_anywhere(&self) -> bool {
        todo!("T07")
    }
```

## 2. `src/reduce/event.rs`: ship the `*_unjoined` placeholders for T07

Golden has the real bodies. The scaffold should ship these three as
placeholders so a T07 learner never edits a T08 stub. The T07 sheet says the
scaffold ships them.

**`take_unjoined`**. Now:

```rust
fn take_unjoined(
    prompts: &mut Prompts,
    actor: &Actor,
    tool: &ToolName,
    hash: InputHash,
) -> Option<Unjoined> {
    todo!("T08")
}
```

Should be:

```rust
fn take_unjoined(
    prompts: &mut Prompts,
    actor: &Actor,
    tool: &ToolName,
    hash: InputHash,
) -> Option<Unjoined> {
    // T08. A placeholder so T07's `dispatch` compiles and runs: nothing is
    // ever parked before T08, so nothing is ever taken.
    None
}
```

**`clear_unjoined`**. Now:

```rust
fn clear_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    todo!("T08")
}
```

Should be:

```rust
fn clear_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    // T08. A placeholder so T07's `end_turn` and `TurnStarted` can call it
    // from the right place now; nothing is parked before T08.
    false
}
```

**`lift_sole_unjoined`**. Now:

```rust
fn lift_sole_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    todo!("T08")
}
```

Should be:

```rust
fn lift_sole_unjoined(prompts: &mut Prompts, actor: &Actor) -> bool {
    // T08. A placeholder so T07's `settle_call` can call it now.
    false
}
```

`#![allow(unused)]` in the scaffold's `lib.rs` covers the unused parameters.
`blocked` stays `todo!("T08")`: `dispatch` reaches it only through
`take_unjoined`'s `Some`, which the placeholder never returns.

## 3. `src/reduce/event.rs`: the `SubagentOutcome` import

Golden does not import it (its `SubagentEnded` arm ignores the outcome), and
adding an unused import to golden would trip clippy. The scaffold has
`#![allow(unused)]`, so it can carry it for a learner whose arm matches on
the outcome. The `use crate::model::{ ... }` block now reads:

```rust
use crate::model::{
    Activity, Actor, Board, Compaction, Decision, Effect, Event, EventKind, Gate, GateKind,
    HistoryEntry, InputHash, Ownership, Prompts, StartSource, Subagent, SubagentId,
    SubagentSource, SubagentStatus, Subagents, Tool, ToolName, ToolOutcome, ToolUseId, TurnKey,
    TurnOutcome, Unjoined,
};
```

Should read:

```rust
use crate::model::{
    Activity, Actor, Board, Compaction, Decision, Effect, Event, EventKind, Gate, GateKind,
    HistoryEntry, InputHash, Ownership, Prompts, StartSource, Subagent, SubagentId,
    SubagentOutcome, SubagentSource, SubagentStatus, Subagents, Tool, ToolName, ToolOutcome,
    ToolUseId, TurnKey, TurnOutcome, Unjoined,
};
```

## 4. `src/reduce/event.rs`: `transition`'s stub text

The scaffold's `transition` has a doc comment golden lacked; golden now has
a longer one (same first paragraph, plus a paragraph saying
`SubagentStarted`/`SubagentEnded` are `transition` arms because they arrive
with `Actor::Agent`). Port that doc comment as is. The stub body's marker
text is fine as it stands:

```rust
    todo!("T07 (agent arms) and T08 (route subagent events, then the permission arms)")
```

but with the split it reads better as:

```rust
    todo!("T07a (session, compaction, cwd arms), T07b (turn and tool arms), T08 (route subagent events, then the permission and subagent arms)")
```

## 5. `src/proc.rs` and `src/reduce/tick.rs`: the `todo!(); std::iter::empty()` pattern

Two functions return `impl Iterator` and the scaffold keeps them compiling
with a dead `empty()`. Replace the marker with a comment saying the
`empty()` is scaffolding, not a starting point.

**`proc::ancestors`**. Now:

```rust
pub fn ancestors(proc_root: &Path, pid: u32, limit: usize) -> impl Iterator<Item = u32> + '_ {
    todo!("T06");
    #[allow(unreachable_code)]
    std::iter::empty()
}
```

Should be:

```rust
pub fn ancestors(proc_root: &Path, pid: u32, limit: usize) -> impl Iterator<Item = u32> + '_ {
    // T06. Replace this whole body. The `empty()` below only keeps the
    // `impl Iterator` signature compiling until you do; it is not a start.
    todo!("T06");
    #[allow(unreachable_code)]
    std::iter::empty()
}
```

**`tick::deadlines`**. Now:

```rust
fn deadlines(agent: &Agent) -> impl Iterator<Item = Instant> + '_ {
    todo!("T09");
    #[allow(unreachable_code)]
    std::iter::empty()
}
```

Should be:

```rust
fn deadlines(agent: &Agent) -> impl Iterator<Item = Instant> + '_ {
    // T09. Replace this whole body. The `empty()` below only keeps the
    // `impl Iterator` signature compiling until you do; it is not a start.
    todo!("T09");
    #[allow(unreachable_code)]
    std::iter::empty()
}
```

## 6. Golden body changes (nothing to port; the scaffold bodies are `todo!()`)

Listed so the diff against golden is not a surprise at grading.

- `src/ingest/claude/transcript.rs`, `user_line`: a prompt with no
  `origin` now maps to `Origin::Unknown` (was `Origin::Human`). The plan's
  test `should_read_an_absent_origin_as_unknown_not_human` states this,
  `Origin::Unknown`'s doc says it, and survey 10 §5 says it; golden's body
  disagreed with all three.
- `src/reduce/event.rs`, `transition`, the `TurnStarted` arm: the duplicate
  key check now runs before `clear_unjoined`, so the transcript's repeat of
  the hook's prompt does not lift a prompt parked between the two reports.
  The plan's test
  `should_keep_a_prompt_parked_between_the_hooks_and_the_transcripts_report_of_one_prompt`
  states this; golden's order failed it.

## 7. Skeleton and sheet facts the scaffold README should agree with

Not my files, for the README owner:

- The README's task table describes T07 as "Reducer I: routing, sessions,
  turns, tools", 2h. Still true; the sheet now splits it into T07a (45m)
  and T07b (1h15) with no renumbering.
- The T07 hidden suite is 33 tests, not 31.
- The T05 sheet now points at survey 10 §5 and §6 only; the README's
  reference line for T05 could say the same.
- The README's reading list item 4 (`docs/02` before T07) is now optional:
  `reduce/mod.rs`'s module comment carries the three conclusions and the
  vocabulary translation.
