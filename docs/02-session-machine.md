# The session machine: phases, writers, and per-child state

Companion to `00-rewrite-plan.md` §4. Records the decision behind the shape of
`Root`/`Child` and the two modelling calls that were put to the design
(approval as a phase; child attention bubbling). Written 2026-09-11 in
response to an external state-machine review of the hook census
(`survey/14-hook-events-verified.md`).

## 0. What the review got right

Three points are adopted wholesale and are load-bearing below.

1. **The turn substate is a map, not a value.** Both agents fire tool hooks
   concurrently for parallel calls, so an active turn holds
   `tool_use_id -> ToolState`. "Thinking" is not a state; it is
   `turn.tools.is_empty() && !turn.stopped`.
2. **`Awaiting approval` has a bypass.** `PreToolUse` goes straight to a
   running tool when no permission is needed, so `PermissionRequest` is the
   only reliable evidence of the approval state. This is fine: it fires
   exactly when a human is needed, which is the only moment the chip cares
   about.
3. **Compaction, subagents, model switches and worktrees are orthogonal
   regions, not substates of the turn.** A compaction is not a kind of
   thinking. Modelling it as one is how "busy" and "idle" got confused.

## 1. Correction: the blind spots are not symmetric

The review states that each agent has exactly one unobservable exit from
`Turn active`, and that they are different ones. The second half is right;
the first is too clean. Verified against the installed binaries:

| Exit | Claude 2.1.268 | Codex 0.154 |
|---|---|---|
| user interrupt **with a tool open** | **observable, exact id** — `PostToolUseFailure{tool_use_id, is_interrupt, error, duration_ms}`; `is_interrupt` is optional in the schema and emitted on the interrupt path | observable via `Interrupt` (root only, no id) |
| user interrupt **with no tool open** | blind. Whether `Stop` fires here is **still unmeasured** — the emitter could not be tied to the abort path by reading the bundle, so the review's "explicitly does not fire" is stronger than the evidence | observable via `Interrupt` |
| turn dies on an API error | observable — `StopFailure{error, error_details}`, main thread only | **blind**. No equivalent event. A 429 leaves the machine in `Turn active` forever |
| failed tool | `PostToolUseFailure` closes the tool by id | no such event; a failed Bash arrives as plain `PostToolUse`, so the *tool* closes correctly but nothing distinguishes the failure |
| process death | blind to every hook; `pidfd` only | same |

So Claude's blind spot is narrow (interrupt during generation, with no tool
open) and Codex's is wide (any API-error death, plus no failure typing on
tools). And both have a third that no hook set can ever cover — a SIGKILL
fires nothing — which is why `pidfd` stays the floor for roots and why
children, which have no pidfd, need mandatory expiry (§4).

### 1.1 Notification timings, verified

- `permission_prompt` fires on a **hardcoded 6 000 ms** timer
  (`NXe=6000`), armed when the prompt opens and `unref`'d.
- `idle_prompt` fires at `messageIdleNotifThresholdMs`, **default 60 000 ms
  but user-configurable**. Never hardcode 60 s in the reducer; read it as a
  delay of unknown length or ignore it.

Both are therefore delayed duplicates of `PermissionRequest` and `Stop`, and
the review's conclusion stands: useful as confirmation of a missed edge,
never as a primary edge.

### 1.2 Claims not verified

- Codex `SessionEnd` firing on 30 minutes of idle rather than on exit. The
  binary yields the event and `end_reason`, but nothing tying it to an idle
  timer was found. If true it is significant — "session gone" and "session
  idle" collapse into one event, and only `pidfd` separates them. Recorded as
  an open measurement (§6), not assumed.

### 1.3 `PostToolBatch` reconsidered, and still not wired

The review is right that `PostToolBatch` is the explicit "map drained, next
model call coming" edge, and that Codex has no equivalent. It stays unwired
anyway, for a product reason rather than a modelling one: the status
vocabulary is `working | idle | permission | unknown`, and thinking and
tool-running are both `working`. The edge buys a distinction that never
reaches a pixel, at one process spawn per batch. Revisit only if a tooltip
ever shows live tool counts.

## 2. Call 1 — approval is neither a turn substate nor a top-level phase

**Decision: it is a separate axis, and `Phase` is not stored at all.**

Promoting approval to a sibling of `Idle` looks right because both mean
"human needed", but it forces a false exclusivity. A `Stop` arriving while a
prompt is open would have to resolve to one of the two, and both answers are
wrong: `Idle` hides a session that needs a human (the false-green class this
rewrite exists to fix), and `Approval` pins a finished session red forever.
The Go daemon already avoids this by storing prompts as *records* keyed by
writer, with the explicit rule that `Stop` never clears a prompt. Keep it.

A phase enum also throws away the field the product needs. Focus-switching
asks "which sessions want me, and who inside each is asking" — a query over
records with owners. `Phase::AwaitingApproval` cannot answer the second half.

So the stored state is a set of orthogonal axes and the chip is a fold:

```
fold order:  permission > blocked > working > delegating > waiting > idle
```

`Phase` becomes a return type, computed on demand, never a field that two
writers can disagree about.

## 3. Call 2 — attention bubbles, activity does not, and the bubble is one-way

**Decision: a child's open prompt raises the root's attention, carrying the
writer. A child's activity does not make the root `working`.**

The argument is physical, not aesthetic. A subagent owns no pane, no window,
no tty and no pid; its permission prompt renders in the *root's* TUI. The
answer to "where do I go" is always the root, so if a child's prompt does not
reach the root's chip the signal is worthless. But the tooltip must name the
child, so the record carries its writer rather than being flattened.

Activity is different. Root idle with children running is `delegating`, an
existing class distinct from `working`; and per `survey/11`, an *idle* team
must make the root `idle`, not `delegating`, which is only decidable from
`TeammateIdle`.

The rule that must be enforced in the types, not in review comments:

> A child may only **raise** the root's attention, never lower it. The root's
> own `Stop` must never clear a child's prompt.

That sentence is the D-35 incident — writer-blind sweeps — stated as an
invariant. Codex `Stop` is deliberately writer-blind and therefore may sweep
*activity* for children, but never *prompts*.

## 4. Per-child state machines

**Decision: yes, but children share the `Activity` machine, not the `Root`
struct, and the map is flat.**

### 4.1 What is good about it

- **One machine, one test suite.** The turn axis is identical for a root and
  a child: start, tools open and close in parallel, stop. Writing it twice
  guarantees they drift.
- **Writer isolation becomes structural.** A prompt owned by a child lives in
  the child's record set, so the root's `Stop` physically cannot reach it.
  The bug class that produced D-35 stops being a discipline problem and
  becomes a type problem.
- **The data model matches the signal model.** Every Claude child hook
  carries `agent_id`, and `SubagentStop` now carries
  `agent_transcript_path` — each child has its own event stream *and* its own
  falsifier file. A per-child machine is what those signals are shaped for.
- **Per-child lifetimes get a home.** The 30-minute stale cap is bolted onto
  the fanout scan today; as a field on the child it is inspectable and
  per-kind tunable.
- **Nesting is real.** `spawnDepth` exists, workflow agents live under
  `wf_*/`, and Codex threads carry `parentThreadId`. Depth > 1 will happen.

### 4.2 What can go wrong

1. **A child is not a session.** If `Child` is `Session`, half its fields —
   pid, pidfd, tty, pane, window, cwd, block — are permanently `None` and
   every consumer must handle a case that cannot occur. *Mitigation: share
   `Activity`, not the struct.*
2. **There is no death primitive for a child.** The root gets a pidfd, an
   exact death signal. A child gets nothing: a crashed subagent, an orphaned
   workflow agent, or a killed teammate emits no terminal event. Evidence
   that this is not theoretical: 55% of background waits in the corpus never
   produce a closing record, and the Go journal has a `failed` line type it
   does not parse — which already pins chips green for 30 minutes.
   *Mitigation: expiry is mandatory per child and per kind, plus a capacity
   bound with LRU eviction of terminal children. A machine with no death
   signal is a leak generator; this session alone spawned eight.*
3. **`agent_id` is not a safe key as it arrives.** It comes bare or prefixed
   and is normalised once; the file stem is `agent-<id>`, and stripping
   `agent-` twice breaks an agent genuinely named `gent-x`; whether a
   teammate's hook id is the file stem or `<name>@<team>` is unmeasured.
   *Mitigation: normalise at the adapter boundary and key on
   `(root_session_id, normalised_agent_id)` so a recycled id in another
   session cannot collide. Never key on the file stem.*
4. **Recursion invites depth and cycles.** A malformed graph — a missing
   parent, a cycle from the app-server — blows the stack on fold or
   serialise, and `state.json` is a flat document (schema v3) anyway.
   *Mitigation: flat `BTreeMap<ChildKey, Child>` with a parent pointer;
   fold iteratively; cap depth.*
5. **Routing ambiguity for id-less events.** Codex does not guarantee
   `agent_id` on child hooks — that is *why* Codex `Stop` is writer-blind.
   Per-child machines force the question the current code dodges, and both
   easy answers are bugs: dropping loses the edge, broadcasting is the sweep
   bug. *Mitigation: routing is a declared per-provider policy in the
   adapter. Claude routes by `agent_id`. Codex root-only events apply to the
   root and may sweep children's **activity** only, never their prompts (§3).*
6. **Combinatorial test surface.** N machines x parallel tools x orthogonal
   regions. *Mitigation: the child machine is a strict subset — no
   compaction, no cwd, no model switch, no block axis — and the fold is
   property-tested rather than enumerated.*
7. **Fidelity the chip cannot render.** Per-child thinking-vs-tool-running
   never reaches a pixel. *Mitigation: keep the child machine deliberately
   coarser than the root's. It exists for three consumers only: the tooltip,
   the `delegating` class, and the history log.*

## 5. Shape

```rust
enum Activity {                       // shared by Root and Child
    Idle  { since: Instant },
    Turn  { started: Instant,
            tools: BTreeMap<ToolUseId, ToolState>,
            last_edge: Instant },     // no `Thinking` variant: tools.is_empty()
}

struct Root {
    ident:      Identity,             // pid, starttime, session_id, provider
    life:       Life,                 // pidfd-backed
    place:      Placement,            // tty, pane, window, focus
    activity:   Activity,
    prompts:    BTreeMap<Writer, Prompt>,
    block:      Block,                // None | UntilTime{at,kind} | Human
    waits:      BTreeMap<WaitId, OpenWait>,
    compacting: bool,
    children:   BTreeMap<ChildKey, Child>,   // FLAT
}

struct Child {
    kind:      ChildKind,             // Subagent | Teammate | WorkflowAgent
    parent:    Option<ChildKey>,
    activity:  Activity,
    prompts:   BTreeMap<Writer, Prompt>,
    last_seen: Instant,
    expires:   Instant,               // mandatory, per-kind
}
```

`Writer` is `Root | Child(ChildKey)`. `Phase` is computed by the fold in §2
and stored nowhere.

Adapters: one `enum ClaudeHook`, one `enum CodexHook`, both mapping into a
shared `enum AgentEvent` with an explicit `Unsupported` variant so a new
event in a future release is a no-op rather than a parse failure. The
reducer is a single `fn step(&mut Root, AgentEvent, now) -> (Observation,
Vec<Effect>)`; the per-agent difference lives entirely in the adapter, in the
routing policy, and in which exits each provider cannot observe.

## 6. Covering the blind spots

`Activity::Turn` carries `stale_after`. A turn is closed by staleness only
when **all** of these hold, and the check runs on the existing 5 s tick:

- no open tools in the map, and
- the transcript has not grown for the window (lines land ~50 ms after their
  own timestamp per `survey/10`, so quiet really is quiet), and
- no CPU delta on the root pid (`utime+stime` from `/proc/<pid>/stat`, which
  is already read for `starttime` — free).

Windows: Claude 90 s (narrow blind spot, and `StopFailure` covers the common
case), Codex 5 min (wide blind spot; the cost of a false close is a green
chip on a dead turn, the cost of a false hold is a green chip on a live one,
so bias long).

The same one-way rule as §3 applies: **staleness may close a turn; it may
never clear a prompt or a block.** A session parked on a permission prompt
has no CPU and no transcript growth and looks exactly like a dead turn.

Codex's binary carries `osPid`, `cpuPercent` and `rssKb` strings, suggesting
it reports process metrics of its own. Unexamined; a lead for the monitoring
feature, not a dependency.

## 7. New open measurements

Added to `survey/14` §5:

14. Does Codex `SessionEnd` fire on an idle timer (30 min?) rather than on
    exit? If so, `end_reason` must never be treated as termination.
15. Does Claude `Stop` fire on an Esc interrupt when **no tool is open**?
    (Item 2 refined: the tool-open case is settled by `PostToolUseFailure`.)
16. Does Codex `Interrupt` fire for an interrupt that lands during an
    approval prompt, or only mid-generation?
17. What is in `Stop.background_tasks[]` and `session_crons[]` — element
    shape is inferred, not captured.
18. For a teammate, is the hook `agent_id` the file stem or `<name>@<team>`?
