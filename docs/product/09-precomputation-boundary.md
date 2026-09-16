# Where to stop computing: the browser boundary

Supersedes `docs/08` §4, which proposed moving the operator model into the
browser. That was wrong for this usage pattern.

## 0. Removing memory

Memory sampling goes. It touches, by reference count: `handler.go` (38),
`internal/provider` (23), `web/app.js` (20), `web/model.js` (21),
`internal/arachne/memory.go` (231 lines) and part of `recorder.go`, the
`memory` provider capability, the `arachne-switchboard-ctl memory` subcommand,
and `EventMemorySample` in switchboard's history vocabulary.

Delete all of it, including the event type. It is the only history event the
daemon emits that describes the machine rather than the work.

## 1. The usage pattern decides the design

Two modes, and they want opposite things:

- **Opened fresh, dozens of times a day.** What matters is the cost of a cold
  start: bytes over the wire, then whatever has to happen before first paint.
- **Left open for hours.** What matters is the cost of an update: ideally
  appending a few nodes, never redoing the day.

Today both are bad. A fresh open fetches a 235 KB envelope and then runs
`model.js` over it; the page then polls every three seconds and **does the whole
thing again**.

## 2. What the browser is actually doing

`model.js` calls itself "pure, DOM-free helpers" that turn a timeline lane into
"a render-ready bar model". That is a complete description of a modelling layer
sitting in the wrong process. Among its 75 functions:

| function | work |
|---|---|
| `packLanes` | interval packing — assigning sessions to swimlane rows |
| `deflickerIntervals(intervals, minMs)` | walks every interval, parameterised |
| `switchArrivals(focusSpans, flickerMs)` | switch-recovery detection, parameterised |
| `awayIdleMs`, `presenceSplitMs`, `workIntervalsMs` | the operator model |
| `normalizedCost`, `apiEquivalentCost`, `costCoveragePercent` | pricing arithmetic |
| `adaptProviderTimeline` | provider adaptation |
| `spanInefficiency` | per-span analysis |
| `buildBar`, `buildBars`, `aloftSpans`, `alignLiveTail` | the render model |

Interval packing and the operator model run on every cold open. `app.js` is
separately 248 KB across 202 functions, but that is rendering and interaction —
hover cards, pinning, the frozen gutter, keyboard shortcuts — which genuinely
belongs on the client.

**The cut is already drawn in the codebase.** `model.js` is the modelling layer
and `app.js` is the view. Moving `model.js` into Rust and making the artifact be
its *output* is the whole change.

## 3. The ladder

How far down to precompute:

| stage | artifact | browser still does | fresh-open cost |
|---|---|---|---|
| 0 | raw events | everything | — |
| 1 | lanes envelope — **today's cut** | parse, model, layout, DOM | parse 235 KB + `model.js` + build DOM |
| 2 | bar model — `model.js` output | layout, DOM | parse + build DOM |
| 3 | geometry in normalised coordinates | DOM only | parse + build DOM |
| 4 | markup | attach handlers | parse HTML, paint |
| 5 | an image | nothing | — loses interaction, reject |

Stages 2 and 3 differ less than they look, provided the SVG uses a `viewBox`
over a fixed domain — the whole day in seconds — and lets the browser scale it.
Then geometry is viewport-independent, precomputable, and zoom or scroll is a
`viewBox` change with no recomputation. Without that, geometry depends on the
container width and cannot be baked.

## 4. The settings objection, and why it does not survive

The reason `docs/08` put the operator model in the browser was that it is
parameterised — away threshold, switch recovery, deflicker window — so baking it
makes the artifact settings-dependent.

But settings change approximately never, and a slider that re-renders instantly
is worth far less than a cold open that does no work. So: key the artifact on the
settings, and regenerate when they change.

```
<day>.segments.json          parameter-free, immutable once final
<day>.<settings-hash>.json   the modelled output, regenerated when settings change
```

The expensive parameter-free stage runs once per day. The parameterised stage
runs once per `(day, settings)` pair and is then also a file. Changing a setting
costs one batch pass over segments that are already computed — offline, not on
open.

## 5. Decision: stage 3, and no further

**Rust owns everything up to normalised geometry. The frontend owns rendering.**

Stage 4 was considered and rejected. Shipping markup would shave tens of
milliseconds off a cold open, and would buy that by coupling the generator to
the DOM: a structural change to the view becomes a regeneration of every
artifact. The boundary should sit where the data stops being data, and that is
at geometry.

So the contract is one artifact per day containing positional geometry in a
fixed coordinate domain, and the client is free to render it however it likes,
change how it looks without touching Rust, and zoom by moving a `viewBox`.

The same shape serves both usage modes: a closed day is an immutable file, and
today is the same file regenerated, with the client patching from a delta
stream rather than refetching.

*Long-open mode* stops polling. The daemon already knows when something changed;
an SSE stream carrying "segments since cursor" lets the client append a handful
of nodes. Re-fetching a whole day every three seconds is the single worst thing
in the current design and it disappears without being replaced by anything
complicated.

## 6. If stage 4 proves annoying

The risk of shipping markup is coupling: a structural change to the DOM means
regenerating artifacts. Styling does not — the 66 KB stylesheet stays
independent, so visual change remains CSS-only.

If that coupling bites, fall back to stage 3 for everything. The cost is
building DOM from geometry on each cold open — tens of milliseconds rather than
near zero — and it is still a large improvement on today, because the modelling
is gone either way. Stage 3 is the safe floor; stage 4 is the optimisation on
top of it.

## 7. What ends up where

| | |
|---|---|
| **Rust, once per day** | annealing, lane packing, the operator model, pricing, geometry |
| **Rust, on change** | today's artifact; the SSE delta stream |
| **Browser, always** | hover cards, pinning, the frozen gutter, keyboard shortcuts, `viewBox` zoom and scroll |
| **Deleted** | `model.js` (91 KB), the 3-second full re-fetch, `/api/timeline` computation, `/api/memory`, the in-memory day cache |
