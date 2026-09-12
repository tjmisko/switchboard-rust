# The dashboard, and how little has to change

`~/Projects/switchboard-dashboard` records and renders a day's history. The
question is what the Rust rewrite owes it. The answer is smaller than it looks,
because the two projects are already decoupled at a seam that has nothing to do
with the daemon's language.

## 1. What actually connects them

Three independent paths feed the dashboard. Only the first involves the daemon.

| Path | Written by | Read by | Daemon involved |
|---|---|---|---|
| `~/.local/state/switchboard/history/<day>.jsonl` | the daemon, append-only | `switchboard-ctl timeline --json` | **yes, and only to append** |
| `~/.local/share/switchboard/summaries/` | `session-digest`, a detached SessionEnd hook | `/api/summaries` | no |
| the cached plan-usage file | the OAuth refresh path | the utilization gauge | no |

And the dashboard does not call the daemon. It runs a subprocess:

```
switchboard-ctl timeline --json --plan-window [--dir D] [--day D] [--since S] [--until U]
```

`cmdTimeline` never opens the socket. It calls `history.ReadRange(dir, from, to)`
and then a chain of pure transforms — swimlane building, suspect intervals, name
backfill, delegation-dormant marking, summarise, aggregate. The daemon's entire
contribution to the dashboard is **appending event lines to a file**.

The README states the decoupling as a design goal: the dashboard reads "through
the stable `switchboard-ctl timeline --json` contract, which keeps it decoupled
from Switchboard's on-disk history format." It is also provider-pluggable —
`internal/provider/config.go` takes a list of `{id, exec, dir, capabilities}`,
and `arachne-ctl` is already a second implementation of the same contract. The
dashboard is a generic timeline viewer that happens to have switchboard as its
default provider.

## 2. `switchboard-ctl` is two programs sharing a binary

| | subcommands | shape | must move to Rust |
|---|---|---|---|
| **live** | `hook`, `focus`, `cycle`, `list`, `pick`, `bottombar watch` | socket clients, latency-sensitive, some on the agent's critical path | yes — they speak the daemon's protocol |
| **analysis** | `timeline` | a pure batch reader over day files; no socket, no concurrency, no long-running state | **no** |

`internal/history/` is ~9,500 lines, roughly 4,000 of them tests. It encodes
accumulated empirical rules — how to merge lanes that never overlap, how to
bound a suspect trailing interval, how to backfill a name across a `/name`, when
a delegating lane counts as dormant. None of that is cheaper to rediscover in
Rust, and none of it is on any latency path: the dashboard shells out to it once
per HTTP request.

So the split is along *coupling*, not language. Keep the analysis in Go.

## 3. The minimum plan

**Phase 1 — the daemon writes history. Nothing else changes.**

`history.Event` is a flat struct of scalars with `omitempty` throughout, plus a
`schema_version`. Emitting it from Rust is a serde struct and an append. The
vocabulary is seventeen types:

```
transition · session_start · session_end · suspend · resume
subagent_spawn · subagent_stop · workflow_start · workflow_stop
usage_sample · usage_cutover · vendor_usage_snapshot
session_label · focus · activity · memory_sample · agent_state
```

Every one already corresponds to something in the model. The mapping is direct
enough to be worth writing down, because it means the history sink is a
subscriber, not a second derivation:

| Event | Source in the Rust model |
|---|---|
| `transition{from,to,dur_prev_ms,subagents}` | `Shown::settle` returning `true`; `dur_prev_ms` is `now - shown.since` |
| `session_start` / `session_end` | `SessionStarted` / agent removal |
| `subagent_spawn` / `subagent_stop` | `SubagentStarted` / `SubagentEnded` |
| `focus` | `Placement::At{focused}` changing |
| `suspend` / `resume` | the `Instant`-vs-`SystemTime` divergence check (`docs/04` §8 / concurrency) |
| `usage_sample`, `vendor_usage_snapshot` | `Snapshot::Limits` — the statusline `rate_limits` feed, which is a better source than the current one |
| `session_label` | `SessionStart{session_title}`, `UserPromptSubmit{session_title}` |
| `activity` | the global idle flag |
| `memory_sample` | not ours; Arachne's recorder emits these |

**Phase 2 — repoint the dashboard. One line, no code.**

The Rust binary will take the name `switchboard-ctl`. Keep the Go analysis half
as `switchboard-history` and tell the dashboard where it is. Both routes are
configuration:

```
switchboard-dashboard -ctl switchboard-history
```

or, in the providers JSON:

```json
{"id":"claude","label":"Switchboard",
 "exec":["switchboard-history","timeline","--json","--plan-window"],
 "capabilities":{"plan":true,"memory":true}}
```

`-ctl` is a flag defaulting to `switchboard-ctl` resolved via `PATH`, and
`exec` is an array in a config file. Neither is a code change. The dashboard
never learns that the daemon was rewritten.

**Phase 3 — nothing.** There is no phase 3. `timeline` stays Go until there is a
reason, and "it is written in Go" is not one.

## 4. What should *not* consolidate

**`session-digest` stays where it is.** It walks transcripts for *content*,
shells out to `claude -p`, runs for tens of seconds, and carries a recursion
guard because its own summarising sessions fire `SessionEnd` too. Putting a
long LLM call inside the process that must never block would be a category
error, and it would breach the privacy boundary the forwarder exists to
maintain: the daemon deliberately never sees prompt or response content, and the
digest is made of nothing else.

The superficial overlap — both walk transcripts — is not real overlap. They read
different fields at different trust levels for different purposes.

**The dashboard itself stays Go.** One binary, assets embedded with `go:embed`,
no frameworks, no network calls. It is a viewer: no latency budget, no
concurrency problem, no cross-platform pressure beyond a browser. The only
argument for rewriting would be sharing types with the daemon, and they do not
share types. They share a file format.

## 5. The real boundary

The version boundary between these projects is the **history record**, not
either binary. `schema_version` is already on the struct, which is the right
place for it.

That gives a rule worth keeping: the daemon may add event types and optional
fields freely; removing a field or changing a meaning is a schema bump, and the
reader has to handle both. Everything else about either project is free to
change independently — which is the property that makes the rewrite cheap.

## 6. One improvement that is nearly free

Today's `transition` records a *status* change. Under the new model there are
two candidates: the computed `Phase` and the displayed `Shown`.

Record `Shown`. The dashboard exists to measure attention — work blocks,
switching cost, time spent supervising — and attention responds to what was on
screen, not to what the daemon concluded internally. A ten-second dwell means
the operator genuinely did not see the state for ten seconds, and a swimlane
that says otherwise is measuring the wrong thing.

The cost is nothing: `settle` already returns whether the visible phase changed,
so the history sink is one subscriber to that boolean.
