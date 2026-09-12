# The history data model, and removing per-request computation

Measured on this machine, 2026-09-11, against a live day.

## 1. Where focus events come from today

Hyprland's event socket reports `activewindow`. That becomes
`wm.EventFocusChanged`, and `applyFocus` runs inside `store.Apply`:

1. Read the prior `Focused` flags to recover which agent session was focused.
2. Set `Focused` on every session by comparing its Hyprland window address to
   the active address.
3. If the focused **session id** changed, record one `EventFocus`.

Two details matter. Change is keyed on session id rather than pid, so a
non-agent window and an agent whose first hook has not landed both read as "no
agent focused" and collapse to one state. And an empty `session_id` is the
signal that closes a focus span.

In the Rust model this is `Placement::At { focused }` changing on any agent,
with the focus edge derived by scanning for the focused one. Same shape, and the
Hyprland adapter already normalises the `0x` address prefix that the two sockets
disagree about.

## 2. What a day costs now

```
switchboard-ctl timeline --json --plan-window
  median 225 ms · 235 KB of output · from a 1.80 MB day file
```

`daycache.go` measures it higher in situ (1.4–2.0 s including the subprocess
spawn) and describes it exactly right: "that fixed cost is the entire perceived
latency of a day switch". It memoises closed days in memory with a TTL, which
helps a revisit and does nothing for a cold load or a restart.

## 3. Where the 1.80 MB goes

3,605 events on the day measured:

| type | bytes | share | events | B/event | kind |
|---|---|---|---|---|---|
| `usage_sample` | 1,346 KB | **74.7 %** | 1,769 | 761 | sample |
| `agent_state` | 205 KB | 11.4 % | 447 | 458 | edge |
| `transition` | 122 KB | 6.8 % | 425 | 288 | edge |
| `focus` | 95 KB | 5.3 % | 816 | 117 | edge |
| everything else | 32 KB | 1.8 % | 148 | — | — |

Three findings, in order of size.

**A usage sample is 761 bytes carrying four numbers.** Its keys are
`agent, cwd, model, pid, project, session_id, tok_cache_create, tok_cache_read,
tok_in, tok_out, ts, type`. Eight of those twelve are constant for the life of
the session and are repeated 1,769 times a day. Three quarters of the history
log is repeated identity strings attached to four counters.

**`transition` and `agent_state` record the same thing twice.** 425 and 447
events, the same `dur_prev_ms` shape, one in the old single-status vocabulary and
one in the three-axis `from_runtime/from_attention/from_lifecycle` form.

**`focus` is the proof.** At 117 B/event it is the one record type that was
deliberately trimmed — the code comment says "focus is minimal-safe" and it
carries no `cwd`. It is 6.5× smaller than the file average, and nothing was lost.

## 4. The redesign

Three changes to the format, none of them a rewrite.

### 4.1 Normalise identity out of every record

`session_id` alone is 36 bytes, repeated on all 3,605 events; `cwd` is longer.
Declare identity once in `session_start` and reference it by a small ordinal
afterwards.

```jsonl
{"t":"session_start","n":7,"ts":"…","session_id":"2a03923e-…","pid":2313575,
 "agent":"claude","project":"arachne","cwd":"/home/u/Projects/Arachne"}
{"t":"focus","n":7,"ts":"…"}
{"t":"state","n":7,"ts":"…","to_runtime":"active","dur_prev_ms":476239}
```

Saves roughly 150–250 bytes on every record — about half the file — before
anything else changes.

### 4.2 Split samples out of the edge log

The same distinction the daemon already draws internally, applied to storage:
edges reconstruct the timeline, samples are aggregated. Mixing them means every
timeline read parses 1.3 MB it only ever sums.

```
history/<day>.edges.jsonl     transition · focus · session_* · subagent_* · activity
history/<day>.samples.bin     usage and memory series, identity by ordinal
```

A sample becomes `(u16 session, u32 ts_delta, u32 × 4 counters)` — 22 bytes
against 761. For the day measured: **38 KB instead of 1,346 KB**.

### 4.3 Drop the duplicate edge

Keep `agent_state`; retire `transition`. One vocabulary, three axes.

**Projected:** 1.80 MB → roughly 150 KB of edges and 40 KB of samples. The
timeline fold reads the edges only, so its input shrinks about **12×**.

## 5. Removing the per-request computation

Smaller input is not the same as no computation. Three steps, in increasing
order of payoff.

### 5.1 Fold the tail, not the day

The history log is append-only, so the fold state at byte offset *N* is a valid
checkpoint for everything before *N*. Persist `{offset, fold_state}` beside the
day file. A request loads the checkpoint and folds `[offset, EOF)`.

With the dashboard's 3 s poll that is typically a handful of events, so the cost
becomes proportional to what is new rather than to how long the day has been.

### 5.2 Materialise closed days to disk

A past day's envelope is a pure function of an immutable input, so compute it
once and keep it: `history/<day>.timeline.json`. A request for a closed day
becomes a file read and a send.

The caveat `daycache.go` already documents is the only subtlety: a session that
started before midnight and is still running keeps extending yesterday's lane.
So a day is *final* only when no session overlapping it is still alive. Until
then serve the materialised copy with a short TTL, exactly as the in-memory
cache does now — but on disk, so it survives a restart and a cold load.

### 5.3 Let the browser keep what cannot change

Once a day is final, serve it with a strong `ETag` and
`Cache-Control: immutable`. Returning to a day already viewed then costs zero
server work and zero network.

**Expected result:** 225 ms per request becomes ~1 ms for a final day (read and
send), a few milliseconds for a recent closed day (checkpoint plus a short
fold), and microseconds for the 3 s poll on today.

## 6. Who does which part

This does not move the analysis into the daemon, and does not contradict
`docs/06`.

| Change | Where | Size |
|---|---|---|
| identity ordinals, split sample stream, drop `transition` | the daemon's history sink | a writer change; the daemon already holds the session identity |
| checkpoint and tail fold | `switchboard-history` (Go) | a cursor around the existing fold |
| materialise final days to disk | `switchboard-history` (Go) | write what the TTL cache already computes |
| `ETag` / `immutable` on final days | the dashboard | a handful of lines |

The ~9,500 lines of swimlane, suspect-interval and name-backfill logic stay in
Go and stay untouched. What changes is what they read and how often they run.

## 7. Migration

The reader must handle both formats for one cycle, which `schema_version` on the
record already allows. Old day files stay readable as they are; new days are
written in the new shape. No conversion pass, no downtime, and the oldest files
age out on their own.

An optional one-off: re-materialise past days once, so the first visit to each is
already a file read.
