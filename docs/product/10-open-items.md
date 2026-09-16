# Open items: what is not written down, and what is not decided

Audit of the design session against `docs/00`–`09`, `src/model.rs`, and the
published pipeline document. Three kinds of gap: decisions taken but recorded
nowhere durable, questions raised and left open, and things never discussed at
all.

Ordered by how much they block. Items marked **blocking** stop a later decision
from being made honestly.

---

## A. Blocking

### A1. The capture harness has never been run — **blocking**

Designed in `survey/14` §5, referenced by half the transition table, and still
hypothetical. Thirteen-plus behaviours are unmeasured, and several of them
decide real structure:

- Does `PermissionDenied` fire on a human decline, or only an auto-mode denial?
  This is the one `Cause::User` the model cannot derive (tab 2 flags it).
- Does `Stop` fire on Esc when no tool is open?
- Does `cost.total_api_duration_ms` advance *during* a turn?
- Is `subagentStatusLine` invoked when the agent panel is closed? If it is only
  invoked while visible, the child table is not a level signal and the whole
  level-triggered subagent design loses its footing.
- What is actually inside `Stop.background_tasks[]` and `session_crons[]`?

One script, both hook configs, one working day. Everything downstream of it is
currently written on inference.

### A2. Codex app-server: the v1 decision predates the evidence — **blocking**

`docs/00` §10.2 recommends hook-only for v1. That was decided before we found
that `turn/completed` carries `{status: completed|interrupted|failed, error}` and
that `account/rateLimits/updated` exists. Those are the *only* sources for two
facts Codex otherwise cannot report at all: a turn that died on an API error, and
usage limits.

Also unresolved: the app-server only reports live state for threads it owns, so
this is a fork between "the daemon launches Codex sessions" and "human-launched
sessions stay hook-only" (`docs/03` §4.2). And `remoteControl`, which announced
itself as `disabled` on the first handshake, has never been examined and might
be a third door.

### A3. `state.json` v3 or v4 — **blocking**

`docs/00` §10.1 left this open when the model was still close to the Go one. It
no longer is. `Shown` versus `Phase`, `Cause`, `GateKind`, `Activity::Unobserved`
and `Lamp::Grey` have no place in v3. Either the new distinctions are dropped at
the publication boundary, or v3 compatibility goes — and §7's "run both daemons
and diff" oracle depends on the answer.

---

## B. Decided this session, recorded nowhere durable

These exist only in the published artifact or in comments in `src/model.rs`.

### B1. Every performance measurement

Taken on this machine, and currently living only in the artifact's tab 5:

| | |
|---|---|
| fold a whole board (16 agents × 4 calls × 8 subagents) | 786 ns |
| seqlock publish / sample, 16 slots | 42 ns / 4 ns |
| atomic write + rename on tmpfs | 19.8 µs (p90 46) |
| `kill(SIGRTMIN+n)` | 1.4 µs |
| one `/bin/sh` per slot update — the current bar config | 1.01 ms |
| `switchboard-ctl timeline --json` | 225 ms, 235 KB out, 1.80 MB in |
| live `state.json` | 11,789 B for 5 sessions |

With the conclusions: the daemon has ~1000× headroom at 60 Hz; the shell in
`waybar/claude.jsonc` is the only real bottleneck; `waybar/circles/module.c`
(CFFI, GIO file monitor) is already the right direction.

### B2. The bar transport recommendation

Sample rather than wait; publish anchors and interpolate locally so the data
rate and frame rate are independent; never resize a chip (a width change
relayouts the whole bar); drive from `gtk_widget_add_tick_callback` and remove
the callback when nothing animates.

### B3. Suspend, and which clock

`Instant` is `CLOCK_MONOTONIC` and does not advance across suspend. A dwell
armed before the lid closed firing on wake is arguably right; a five-minute
staleness cap never firing is wrong. Detection via `SystemTime` delta versus
`Instant` delta is in the artifact — **the decision is not made**: whether stale
caps should use `CLOCK_BOOTTIME` directly rather than being corrected after the
fact.

### B4. Reconstruction may never produce `Attention`

After a daemon restart the transcript can prove a prompt resolved but not that
one is open, so replaying a day would turn old prompts into a burst of red.
Reconstructed state enters at `Confidence::Inferred` and cannot reach the top of
the ladder. Stated in the artifact, absent from `docs/`, and it is a rule the
reducer must actually enforce.

### B5. `GateKind { Approval, Question }`

In `src/model.rs` only. `AskUserQuestion`, `ExitPlanMode`, `request_user_input`
and MCP elicitations are tool calls that *are* the question and never fire a
permission event — which is exactly why the Go matcher special-cases two tool
names. A reducer watching only for approvals misses them.

### B6. The federation facts read out of the Go repo

In `src/model.rs` comments, nowhere in `docs/`:

- `Hostname`, `Remote`, `Navigable` and `LocalWorkspace` are populated **only**
  on detached aggregate copies; the host-local store leaves them empty so the
  durable schema is unchanged.
- `StartedAt` is the daemon's discovery-lifetime fence, explicitly *not* a
  kernel process-birth token — because there is no remote `/proc`.
- `DropRemoteHost` forgets routes at the disconnect edge specifically so a
  remote daemon that restarts and reuses a `StartedAt` cannot resurrect a stale
  route.
- Remote frames are whole snapshots with a 4 MiB cap; a wakeup means re-read
  everything, never apply a delta.

### B7. The corpus is invisible to default tooling

`rg` and `fd` skip `~/.claude/projects` without `-uuu` — a global ignore rule,
not just hidden-file handling. Every measurement in this session was initially
wrong because of it. Any script that scans the corpus must bypass ignores, and
that belongs in a comment next to the scanning code.

---

## C. Never discussed

### C1. `switchboard explain` — why is this chip that colour?

The daemon's entire job is inference over partial evidence, and there is
currently no way to ask it why it concluded something. When a chip is wrong —
which is the failure this rewrite exists to reduce — the only recourse is
reading logs.

The pure-fold design makes this nearly free. Keep a bounded ring of recent
inputs per agent; the fold can then be re-run over them to produce an evidence
chain: which event set which axis, which snapshot was superseded, which dwell is
pending and when it expires. This is probably the highest-value feature nobody
has mentioned, and it costs a `VecDeque` and a formatting function.

### C2. Deterministic replay as the test strategy

`docs/00` §7 proposes running both daemons and diffing. Replay is cheaper and
stronger: inputs are serialisable and `apply_event`/`apply_snapshot`/`tick` are
pure, so a recorded day replays into a state trajectory that can be diffed
against a golden one. It is also the natural regression test for a harness
release — replay yesterday against the new adapters and see what moved.

Needs: a stable serialisation for `Input`, and a decision about whether the
capture harness's output *is* that format.

### C3. Hook installation, and writing the user's config

Capability is *emitted ∧ subscribed*, so the daemon must read
`~/.claude/settings.json` and `~/.codex/hooks.json`. The v1 hook set is nineteen
Claude events and twelve Codex ones. Nobody is hand-editing that, and it changes
with every release that adds an event.

So the daemon should probably offer to write those files — which is a
destructive edit to user configuration and needs a backup, a diff shown before
writing, and idempotence. Never discussed, and it is the first thing a new user
hits.

### C4. Coexistence and migration with the Go daemon

Both bind the same socket, write the same `state.json`, and append to the same
history directory. There is no plan for running them side by side, taking over,
or rolling back. At minimum: a distinct socket path during development, a lock
file, and a documented takeover sequence.

### C5. CLI surface compatibility

`waybar/claude.jsonc` runs `switchboard-ctl focus idx:0`, `cycle prev`,
`cycle next` on click and scroll; `claude-picker` and `claude-abbrev-edit` shell
out too. Those break the moment the Rust binary takes the name without keeping
the surface. No compatibility commitment has been written down.

### C6. Time zones and the day boundary

History is bucketed by local day. Under DST a local day is 23 or 25 hours, which
**breaks the `[0, 86400]` viewBox domain specified for the stage-3 artifact** in
`docs/09`. Also unresolved: what a session spanning a DST change does to
`dur_prev_ms`, and whether history should be UTC-bucketed with local rendering.

### C7. Cost and pricing

`internal/pricing` is a real package in the Go daemon, and cost cards are a
headline feature of the dashboard. The new design has never said where pricing
lives: the model→price table, cache read versus write rates, what
`usage_cutover` means (15 events a day), or whether the daemon computes cost at
all or only emits token counts and lets anneal price them.

The last framing is probably right — pricing tables change without the
daemon changing — but it has not been decided.

### C8. Three history event types are unmapped

`workflow_start` / `workflow_stop` and `usage_cutover` appear in the history
vocabulary and in no mapping table. `activity` is mapped but its *source* —
swayidle, an RPC input in the Go daemon — does not appear in the Rust input
taxonomy at all, despite feeding the dashboard's entire operator model.

### C9. Project naming and abbreviation

The daemon resolves cwd to a project abbreviation; `project_full` is an optional
pretty name; `claude-abbrev-edit` lets the operator override per slot; `/name`
writes name spans that the dashboard renders along a bar. None of this is in the
model, which has only `workspace: Option<Box<str>>`.

### C10. Desktop notifications

Should the daemon notify, not just colour a chip? The cost model has a great deal
to say about this — a notification is a far louder interrupt than a lamp, so the
dwell and ownership rules should be *stricter*, not the same. Untouched.

### C11. tmux, multi-monitor, multiple WM instances

The Go daemon supports tmux panes; only wezterm is modelled. Nothing says what
happens with two monitors, or a second Hyprland instance.

### C12. Trust boundary

Any process running as the user can connect to the socket and claim to be any
session; the daemon's ppid walk to the nearest tracked agent is the actual
authentication. That is probably adequate — the boundary is the user account —
but it has never been stated, and remote frames are untrusted input that must be
parsed defensively.

---

## D. Operational, not yet touched

| item | note |
|---|---|
| daemon lifecycle | systemd unit, restart policy, socket activation, ordering against the bar |
| history retention | ~1.8 MB/day now, ~190 KB after the `docs/07` format fix; still ~70 MB/year with no rotation policy |
| resource bounds | max agents, max subagents, inbox depth, behaviour at N=100 agents |
| logging | levels, destination, and how a wrong chip gets diagnosed without `explain` |
| unknown-version reporting | the `Unsupported` path exists; counting it and surfacing "your harness emits events we ignore" does not |
| packaging | cross-compilation for macOS, and what ships with what |

---

## Suggested order

1. **A1** — run the capture harness. Everything else is inference until it runs.
2. **C1** — `explain`. Cheap now, and it makes every later decision debuggable.
3. **A3**, then **A2** — the two structural decisions.
4. **C2** — replay, which turns the capture output into a test corpus.
5. **B1–B7** — write down what is already decided, before it is lost.
6. **C3–C5** — the migration path, before any of it runs on this machine.
