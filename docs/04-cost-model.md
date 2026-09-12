# Cost model

What the chip is for, what it costs when it is wrong, and the design rules
that follow. Everything downstream — hysteresis, dwell times, which signals
may raise a state and which may lower it — derives from this document.

## 1. The chip is an interrupt, not a display

A dashboard reports. An interrupt competes for attention that is currently
elsewhere. That is the whole cost model: the chip's job is to be worth looking
at, and every time it is wrong in the direction of "look at me" it spends
credibility it cannot easily earn back.

## 2. Two dimensions, not one

The naive model is that being wrong upward costs more than being wrong
downward. That is true but incomplete. The real variable is **whether the user
already knows.**

A chip is wrong in one of two ways:

- **False alert** — we say you are needed; you are not. Cost is the broken
  focus, paid in full the moment you look, plus a durable increase in the odds
  you ignore the next one. Scales with **occurrence**, not duration: a
  ten-second flash costs nearly what ten minutes costs.
- **False calm** — we say you are not needed; you are. Cost depends entirely
  on whether the user holds contradicting knowledge:
  - **Unobserved.** You are working elsewhere and have no independent view of
    that agent. Cost accrues slowly with duration. Ten seconds is free; an
    hour is bad.
  - **Contradicted.** You just hit Escape three times. You *know* it stopped.
    A chip still showing green is not late — it is visibly lying, and it costs
    what a false alert costs, immediately, for the same reason: it teaches you
    the chip cannot be trusted.

So:

```
cost = Σ over false alerts        ( interruption + trust_decay )
     + Σ over contradicted calms  ( trust_decay )
     + λ · ∫ unobserved false calm dt          (λ small)
```

Only the last term is a rate. The other two are counts, and counts dominate.

## 3. The decisive question: did the user cause it?

A transition the user caused is one they already know about. Showing it is
confirmation, and **confirmation is not news** — it costs nothing to deliver,
and withholding it is what costs.

A transition the agent caused is news. Delivering it early and wrong is the
expensive mistake.

This cuts across the severity ladder, and it produces the only table that
matters:

| | **User caused it** (they know) | **Agent caused it** (they do not) |
|---|---|---|
| **Escalate** | immediate — confirmation, never news | dwell, per evidence quality (§5) |
| **De-escalate** | immediate | immediate |

Three of the four cells are immediate. Hysteresis exists for exactly one cell.

The Escape case is the reason this table is shaped that way. Interrupting a
turn moves the chip *up* the ladder — green to orange — so a naive
escalate-slowly rule would delay it. But the user pressed the key. There is no
one to surprise, and a chip that stays green for ten seconds after they stopped
the agent is the most corrosive failure in the system, because it is the one
failure they can personally verify.

Events that are user-caused, and the field that proves it:

| Signal | Why it is user-caused |
|---|---|
| transcript line with `interruptedMessageId` | Escape during generation |
| `PostToolUseFailure{is_interrupt: true}` | Escape during a tool |
| `toolDenialKind: "user-rejected"` | declined a prompt |
| Codex `Interrupt` hook | Ctrl-C |
| `UserPromptSubmit` with `origin.kind: human` | typed a prompt |
| `SessionStart{source: "clear"}` | `/clear` |
| any approval answered | answered a prompt |

Everything else — rate limits, API failures, tool completion, subagents
finishing, staleness — is agent-caused.

## 4. Flapping is worse than either error

green → red → green → red inside a minute is worse than two independent false
alerts, because it also destroys the ability to predict the signal. A chip that
changes while you are looking at it teaches you to stop looking.

The quantity to minimise is **agent-caused transitions into an attention
state**, not time-in-state and not error rate.

## 5. Rules

**R1 — Only agent-caused escalation waits.** Everything else is immediate. See
the table in §3.

**R2 — Dwell length is a function of evidence quality, not of the target
state.** A dwell only buys something when the evidence might be withdrawn.

| Evidence | Dwell before escalating |
|---|---|
| Anything user-caused | **0** |
| Confirmed event: exact, positive, owned — a Claude `PermissionRequest`, where every prompt is human; an app-server approval request with `approvalsReviewer: user` | **0** |
| Inferred: an unowned gate past its grace, a block read off a transcript error line, a turn closed by staleness | **10 s** |
| Snapshot-derived: the subagent panel reporting a blocked child | **one refresh interval + ε**, so a stale snapshot cannot raise |

Ten seconds of *unobserved* false calm is inside the free region of §2. It is
the cheapest insurance available — and it is never spent on the contradicted
case, because those are all user-caused.

**R3 — Never escalate on inference alone into the top of the ladder.** A gate
whose owner was never established renders `Unknown`, never `Attention`. This is
what `Ownership` is for, and Codex is the case that needs it.

**R4 — Liberal in what lifts, conservative in what raises.** Any credible
evidence of resolution clears an alert immediately: `PostToolUse`,
`PermissionDenied`, `PostToolUseFailure`, a transcript `tool_result`, a
transcript `interruptedMessageId` line, `Stop`, the next `UserPromptSubmit`,
`turn/completed`, `serverRequest/resolved`, a panel row that stopped being
blocked. Only a small, exact set may raise one.

The Go daemon has this backwards: a pending prompt is re-asserted until a
*matching* release arrives, so a prompt can stay red well past the moment it
was answered. Answering a prompt is user-caused, so the user knows — which
makes a stale red a contradicted state under §2, and the most expensive single
bug in the system.

**R5 — When evidence is ambiguous, hold the current state.** Do not oscillate
to express uncertainty. `Unknown` is for a gate we can see but cannot
attribute, not for "the last signal confused me".

**R6 — Separate semantic hysteresis from publish coalescing.** Movement inside
the calm band interrupts nobody and needs no dwell. It does cause writes and
Waybar signals, so coalesce on a short timer (~250 ms). That is an I/O concern
and must not be confused with R1.

## 6. The ladder, and the colour that is not on it

```
alert  (red)     Attention
notice (amber)   Stopped · Blocked
calm   (green)   Idle · Working · Delegating · Waiting
                 Unknown  -> grey
```

Severity and colour are different axes. Severity is what hysteresis governs.
Colour is what the operator reads.

`Unknown` is grey, and grey is free in *both* of §2's terms: it interrupts
nobody, so it cannot be a false alarm, and it asserts nothing, so it cannot be
contradicted. That makes it the only honest answer in the three situations
where we have no answer:

- an agent discovered before we ever heard from it, which is every agent when
  the daemon starts after them;
- a gate we can see but cannot attribute to a person (Codex without the
  app-server);
- a federated row whose host has stopped answering, where the phase is a
  memory of what another machine believed.

Grey therefore sits at `Severity::Calm`, so green and grey move between each
other freely in both directions. Treating it as an escalation would delay the
admission that we do not know; treating it as a de-escalation would delay the
return once we do.

The temptation is to show green instead — "probably fine". That is a claim,
and the third case above is exactly where the operator is most likely to know
better.

`Stopped` is a phase in its own right: a turn the user interrupted, parked with
work unfinished. It is not `Idle`, which means the agent finished what it was
asked and is waiting for the next thing. The difference is a pending
obligation, and after you walk away and come back it is exactly the difference
you want to see.

`Blocked` sits in notice rather than alert because a rate-limited agent wants
nothing from you; telling you is a courtesy so you can go elsewhere. Waking you
for it would be a false alert by §2.

This matches the current Go implementation, where `unknown` is never stored and
renderers synthesise it from an empty status — grey is already what an
un-enriched session looks like there.

Within the calm band there is no ordering for hysteresis purposes; movement is
free.

## 7. What this changes in the model

`Phase` is derived — a pure function of the axes, and it cannot drift from
them. But **what the chip shows is not `Phase`**. The displayed state has
memory: what it currently shows, since when, and whether a higher phase is
waiting out a dwell.

```rust
Agent::phase(&self, limits) -> Phase        // the truth now, no memory
Shown { phase, since, pending }             // what the user sees, with memory
Shown::settle(truth, cause, now)            // the only place the cost model lives
```

`settle` is small enough to test exhaustively, and the `cause` argument is what
makes §3 enforceable rather than aspirational.

This resolves an earlier argument. The objection to a stored `status` was that
several transitions would write it and eventually disagree. That still holds
for `Phase`. It does not hold for `Shown`, because hysteresis genuinely
requires memory — and that memory has exactly one writer.

## 8. Latency budgets

Because unobserved lateness is cheap and both contradiction and false alerts
are not:

- Transcript watching must be inotify, not a 5 s poll. Every user-caused signal
  that has no hook — Escape during generation, in particular — arrives only
  there, and those are the contradicted cases.
- The 30 s Codex approval grace is a dwell under R2 and is defensible, but it
  must render `Unknown` on expiry, never `Attention`.
- A staleness sweep that closes a turn may run on a slow tick: it only moves
  state down, which is the cheap direction.
- Discovery latency is nearly free. An agent that appears 10 s after it started
  costs nothing.

## 9. Not covered here

Aggregate display: how many chips, in what order, and whether one "someone
needs you" indicator should sit above them. The answer probably follows the
same asymmetry.
