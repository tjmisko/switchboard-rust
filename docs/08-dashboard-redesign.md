# A lighter dashboard: write, anneal, read

The proposal — switchboard writes a history, a small postprocessor repairs and
materialises it, the dashboard just reads — is the right shape. This records
what the dashboard currently does, what each responsibility becomes, and the
order to do it in.

## 1. What the dashboard does today

Six routes:

| route | what it does |
|---|---|
| `/api/timeline` | spawns one provider subprocess per source, merges the envelopes, memoises closed days in memory with a TTL |
| `/api/memory` | same fan-out, for the memory series |
| `/api/plan` | reads a cached OAuth plan-usage file |
| `/api/summaries` | lists records written by `session-digest` |
| `/api/settings` | reads **and writes** the operator model (away threshold, switch recovery) |
| `/` | serves 638 KB of embedded assets |

And three things live in the repo that are not the dashboard:

| package | lines | what it really is |
|---|---|---|
| `internal/arachne` | 4,024 | a cgroup/docker/memory recorder for a different project, which happens to implement the same `timeline --json` contract |
| `internal/sessiondigest` | 2,604 | an LLM summariser with its own binary and its own SessionEnd hook |
| `internal/timeline` | 1,942 | cross-provider merge and suspect handling |

The decisive measurement is on the client: `web/model.js` is 91 KB across 75
functions and `web/app.js` is 248 KB across 202. **The browser is already the
analysis engine.** The Go server is mostly a pipe that spawns subprocesses and
forwards their output — and the expensive part of a request is the subprocess it
spawns, not anything it computes itself.

## 2. The shape

```
switchboard daemon ──▶ edges + samples          append-only, live, lossy
                          │
                     anneal (batch)             retrospective repair + materialise
                          │
                          ▼
                   day artifacts                immutable once final
                          │
                     static serving
                          │
                          ▼
                   browser model                parameters applied at render
```

Each stage is boring on its own, which is the point.

## 3. Annealing is where retrospective truth belongs

This is the strongest argument for the design, and it is not merely about
caching.

The live daemon is bound by an asymmetry: the transcript can prove a prompt
*resolved* but never that one is *open*, because a blocked writer and a busy one
are the same bytes on disk. That is why reconstruction after a restart may never
assert attention — it would replay old prompts as a burst of false alarms.

**Retrospectively that constraint disappears.** For a past interval you know how
it ended, so you can say with confidence that a session was blocked from 14:03
to 14:11. A batch pass over a finished day can therefore repair things the
daemon structurally cannot:

- close spans left open by a crash, a restart, or a missed `Stop`
- fill windows where the daemon was not running at all
- correct `dur_prev_ms` across a suspend
- recover blocked intervals that only the transcript reveals
- reconcile subagent lanes against `subagents/` and the parent's tool-use ids
- fold the usage samples into per-session totals once

And it *deletes* a category of code from the read path. `internal/history/suspect.go`
is 317 lines with 797 lines of tests, and it exists because a live reader cannot
tell whether a trailing state is real. After the fact it can. Suspicion is a
live concept; annealed days do not need it.

Annealing also wants exactly what the daemon must never touch: transcript
content, unbounded time, and permission to be slow. Putting it in a separate
batch pass is not a compromise, it is the correct home.

## 4. Split the analysis by parameter-dependence

The one thing that stops a day artifact from being fully immutable is that some
of the analysis is parameterised by settings.

| | depends on | cost | where |
|---|---|---|---|
| lanes, spans, focus intervals, session identity, token and cost totals | the log only | expensive: parsing and merging | **anneal**, once |
| work blocks, away detection, switch recovery, the operator lane | the log **and** the away threshold / switch-recovery settings | trivial: arithmetic over a few hundred segments | **browser**, per render |

Cutting there means the expensive half is materialisable and the parameterised
half is free. It is also better to use: moving a threshold slider re-renders
immediately instead of round-tripping to recompute a day.

That also retires `/api/settings` as a server concern. If settings only
parameterise client-side arithmetic, they are per-viewer preferences and belong
in the browser.

## 5. What each responsibility becomes

| today | after |
|---|---|
| `/api/timeline` — subprocess, merge, TTL cache | a file read of `<day>.timeline.json`; merge happens once at anneal, or in the browser across N artifacts |
| `/api/memory` | another artifact from the same pass |
| `/api/plan` | a small file in the artifact directory |
| `/api/summaries` | `session-digest` already writes files; point at them |
| `/api/settings` | browser-local |
| static assets | unchanged |

The end state is a **directory of JSON artifacts and a static file server**. The
dashboard backend can then be anything, including nothing: `switchboard-ctl serve`
as a convenience, or a generic server pointed at the directory.

The three lodgers move out on their own timetable. `internal/arachne` is a
different product that already speaks the provider contract, so it can become
its own binary writing its own artifacts. `session-digest` is already a separate
binary with its own hook. Neither needs the dashboard to exist.

## 6. Order, so that each step stands alone

1. **Fix the write format** (`docs/07`). Identity ordinals, samples split from
   edges, drop the duplicate edge. 1.8 MB/day becomes ~190 KB. No dashboard
   change, since the reader handles both versions for one cycle.
2. **Add `anneal`** to the existing Go analysis binary:
   `switchboard-history anneal --day D` writes `<day>.timeline.json`. This is
   the pipeline that already exists with a different output sink — nearly no new
   logic — plus the transcript repair pass, which is the only genuinely new
   work.
3. **Point `handleTimeline` at the artifact directory.** One function becomes a
   file read. Per-request computation is gone at this point; everything after is
   cleanup.
4. **Move the operator model into the browser.** Additive to `model.js`, which
   is already doing this kind of work.
5. **Delete what is left of the backend**, or keep a static server. By now it
   serves files and nothing else.

Steps 1–3 remove the 225 ms per request. Steps 4–5 remove the reason for a
backend at all.

## 7. What this costs

The honest risks, none of them large:

- **A format version between anneal and browser.** Real, but it is one producer
  and one consumer in the same repository.
- **Re-annealing after a bug fix.** It is a batch job over immutable input;
  re-running a month is a loop.
- **Today still needs frequent work.** Handled by the checkpoint in `docs/07`
  §5.1: fold the tail, not the day.
- **A day is final only when every session overlapping it has ended**, which
  `daycache.go` already documents. Until then the artifact is served with a
  short TTL rather than as immutable.
