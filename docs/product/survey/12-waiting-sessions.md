# Survey 12 — Sessions waiting on monitors, timers and background tasks

Date: 2026-09-11. Read-only. Corpus: 2,376 transcript files (1.2 GB, 303,800
lines) under `~/.claude/projects`, Claude Code versions 2.1.229 → 2.1.268;
plus `rg -a` over the installed 2.1.266/267/268 binaries; plus the Go daemon
at `~/Projects/switchboard`. Privacy: structural fields, enum values and
sentinel strings only; every quoted value is trimmed and ids are elided.

## 0. The problem and the one-line answer

A root that arms a background wait (background Bash, Monitor, `/loop`
ScheduleWakeup, CronCreate, a background subagent, a Workflow) ends its turn
normally: `stop_reason == "end_turn"`, the `Stop` hook fires, the chip goes
orange. The harness later re-invokes the model with a synthetic `user` line
and no human touched anything. Today only the subagent case (`delegating`)
stays green.

**Headline finding.** Since at least 2.1.266 the `Stop` and `SubagentStop`
hook payloads carry two arrays the Go daemon has never read:

```
background_tasks: [{ id, type, status, description, command?, agent_type?, server?, tool?, name? }]
session_crons:    [{ id, schedule, recurring, prompt }]
```

with `type` a friendly label ("shell", "subagent", "monitor", "workflow", …).
The schema's own description: *"Lets hooks distinguish 'session is done' from
'session is paused waiting for background work to wake it'. Empty array when
nothing is in flight."* `session_crons` covers CronCreate, ScheduleWakeup and
`/loop`. `switchboard-ctl`'s `hookPayload` struct
(`cmd/switchboard-ctl/main.go:643-660`) has no field for either, so the
daemon drops them on the floor. The design in §3 is hook-first with the
transcript pairs of §1 as the fallback and the cross-check.

## 1. How the arming tools appear in transcripts

All counts are over the whole corpus (main transcripts + `subagents/`).
`toolUseResult` is the top-level field on the `user` line that carries the
`tool_result` block; it is the structured twin of the text ack.

| Tool (as classified) | n | input key sets (n) | ack text (trimmed) | `toolUseResult` keys |
|---|---|---|---|---|
| `Bash` with `run_in_background: true` | 466 | `command,description,run_in_background` (350); `+timeout` (107); `command,run_in_background` (9) | `Command running in background with ID: b…. Output is being written to: /tmp/claude-1000/<proj>/<sess>/tasks/b….output` | `backgroundTaskId, interrupted, isImage, noOutputExpected, stderr, stdout` (+`backgroundCwdHint`) |
| `Monitor` | 98 | `command,description,persistent,timeout_ms` (95); 3 malformed (InputValidationError) | `Monitor started (task b…, timeout 3600000ms). You will be notified on each event…` (86) / `Monitor started (task b…, persistent — runs until TaskStop or session end)…` (6) | `persistent, taskId, timeoutMs` |
| `ScheduleWakeup` (`/loop`) | 2 | `delaySeconds,noop,prompt,reason` (1); `stop` (1) | `Next wakeup scheduled for HH:MM:SS (in Ns)…` / `Loop stopped — cancelled N pending wakeup(s)…` | `clampedDelaySeconds, scheduledFor, wasClamped` (+`cancelledWakeups, stopped`) |
| `CronCreate` | 10 | `cron,prompt,recurring` | `Scheduled one-shot task <8hex> (m h d M *). Session-only (not written to disk, dies when Claude exits)…` (9) / `Scheduled recurring job <8hex> (Every N mi…` (1) | `durable, humanSchedule, id, recurring` |
| `CronList` / `CronDelete` | 2 / 1 | `{}` / `id` | `<id> — … (one-shot) [session-only]: …` or `No scheduled jobs.` / `Cancelled job <id>.` | `jobs` / `id` |
| `TaskStop` | 49 | `task_id` | JSON `{"message":"Successfully stopped task: <id> (<command>)",…}` | `command, message, task_id, task_type` |
| `TaskOutput` | 6 | `block,task_id,timeout` | `<retrieval_status>timeout|not_ready</retrieval_status>` or `No task found with ID` | string error or status |
| `Agent` (no `run_in_background`) | 215 | `description,name,prompt,subagent_type` (144) + `isolation`/`model` variants | `Spawned successfully. (This tool result is internal metadata…` (162, teammates) / `Async agent launched successfully…` (46, background subagents) / `Fork started — processing in background` (1) / 3 inline results | teammates: `agent_id, agent_type, color, is_splitpane, model, name, plan_mode_required, prompt, status, team_name, teammate_id, tmux_*` |
| `Agent` with `run_in_background: false` | 1 | | `Spawned successfully…` | same |
| `Workflow` | 110 | `script` / `scriptPath` / `args` / `resumeFromRunId` combos | `Workflow launched in background. Task ID: w…` (94); 7 denied/invalid | `runId, scriptPath, status, summary, taskId, taskType, transcriptDir, workflowName` |
| `SendMessage` | 240 | `content,message,recipient,summary,to,type` | JSON `{"success":true,"message":"Message sent to <name>'s inbox","msg_id":…}` | `message, msg_id, routing, success` |

Confirmed again (cf. `docs/async-agent-launch-ack.md`): `Agent` has no
`run_in_background` input in practice (215 absent, 1 explicit false, 0 true);
"background-ness" is proven by the ack text, never by the input.

**Task-id families** (the join key everywhere): `b` + 8 `[0-9a-z]` for both
shell and monitor tasks (they are not distinguishable by id); `w` + 8 for
workflows; `a` + 16 hex for subagents; 8 hex for cron jobs; a UUID `msg_id` for
messages. `tool_use_id` is the better key: every terminal notification but the
monitor events carries `<tool-use-id>`.

Sessions: 69 main transcripts armed at least one wait; 58 of those left at
least one arm with no closing record of any kind (§1.4).

### 1.1 The wake-up: `<task-notification>`

One `user` line, `message.content` a **string** (never a block list):

```
<task-notification>
<task-id>b…</task-id>
<tool-use-id>toolu_…</tool-use-id>
<output-file>/tmp/claude-1000/<proj>/<sess>/tasks/b….output</output-file>
<status>completed</status>
<summary>…</summary>
[<result>…</result>] [<note>…</note>] [<usage>…</usage>] [<worktree>…</worktree>]
</task-notification>
```

Tag presence by arm kind (n = notifications matched to an arm by id):

| arm kind | n | task-id | tool-use-id | status | output-file | result | event | note |
|---|---|---|---|---|---|---|---|---|
| BackgroundBash | 57 | 57 | 57 | 57 | 56 | 0 | 0 | 1 |
| Workflow | 97 | 97 | 96 | 97 | 89 | 87 | 0 | 2 (+diagnostics, recovery, failures) |
| Agent (background subagent) | 37 | 37 | 36 | 37 | 37 | 36 | 0 | 37 (+usage 6, worktree 5) |
| Monitor | 112 | 112 | 5 | 6 | 4 | 0 | 106 | 1 |

`<status>` values seen: `completed` (183), `stopped` (10), `failed` (9),
`killed` (9). **Monitor events carry no `tool-use-id` and no `status`**: the
shape is `<task-id>`, `<summary>Monitor event: "<description>"</summary>`,
`<event>…</event>`. A monitor's end is itself an event: `<event>[Monitor timed
out — re-arm if needed.]</event>` (17 in corpus), `[Monitor stopped]` (3), or
a `<status>stopped|completed</status>` line (6, when killed by TaskStop or the
command exited).

Top-level fields that distinguish the wake-up from a human prompt:

| field | task-notification (main thread) | human prompt | cron fire (2.1.247) | teammate message |
|---|---|---|---|---|
| `origin` | `{"kind":"task-notification"}` (338) | `{"kind":"human"}` (696) | absent | absent |
| `promptSource` | `"system"` | `typed` (637) / `queued` (39) / `suggestion_accepted` (8) | `"system"` | absent (4 `sdk`) |
| `isMeta` | absent | absent | `true` | absent |
| `queueSkipAttachments` | `true` (153) | absent | — | — |
| `queuePriority` | `"later"` (49) | absent | — | — |
| `queueOrigin` | `{"kind":"task-notification",…}` (3) | — | — | — |
| `scheduledTaskId` / `scheduledFireId` | — | — | present (18) | — |
| content | string starting `<task-notification>` | string or block list | string, the stored prompt | string `Another Claude session sent a message:\n<teammate-message teammate_id=… color=… [summary=…]>` |

Other `origin.kind` values in the corpus: `peer` (43, cross-session
`SendMessage` bodies), `auto-continuation` (3). The 2.1.268 binary's origin
schema adds `subkind ∈ {scheduled-trigger, peer-send-message,
projects-relay}` and `source` (`goal-checkin`, `worker-checkin`,
artifact-watch), so a cron/loop fire on a current build is
`origin = {kind:"task-notification", subkind:"scheduled-trigger"}`.

**Rule for a transcript reader:** `origin.kind == "task-notification"` or
`promptSource == "system"` → machine wake-up; `origin.kind == "human"` or
`promptSource ∈ {typed, queued, suggestion_accepted}` → human. Do not key on
`isMeta` (absent on main-thread notifications) or on `isSidechain`.

### 1.2 Sibling forms

- **Teammate idle report** (218 in subagent files, 171 embedded on main
  threads): `<teammate-message>` wrapping JSON
  `{"type":"idle_notification","from":…,"idleReason":"available","timestamp":…[,"summary"|"result"]}`.
  `idleReason` is always `available` (408). This is how a teammate's turn end
  reaches the lead; it carries no `origin` and no `promptSource`.
- **Subagent-side notification** (7, versions 2.1.232–2.1.246): a subagent
  that armed its own background task receives `[SYSTEM NOTIFICATION - NOT USER
  INPUT]\n…` followed by the same `<task-notification>` block, with
  `isMeta: true`, `isSidechain: true`, `origin.kind: task-notification`.
- **Scheduled fire** (2.1.247, one recurring `*/20` job, 26 fires): a
  `system` line `{subtype:"scheduled_task_fire", content:"Running scheduled
  task (…)", taskId, cron, prompt}` immediately followed by the `user` line
  (`isMeta:true, promptSource:"system", scheduledTaskId, scheduledFireId`).
  Nothing in the corpus shows a `/loop` wakeup actually firing: the one
  ScheduleWakeup (delay 1500 s) was cancelled with `{stop:true}` 22.5 min
  later; the harness's own `loop_wakeup_aged_out` telemetry key says loops
  also expire on their own.
- **Bulk kill**: `system` line `subtype:"agents_killed"` (18) followed by a
  `user` line `"N background agents were stopped by the user: …"` with no
  XML and `origin.kind: task-notification` — the 21 "unmatched" notifications
  in the scan.
- **SDK stream only** (not on disk): `system/background_tasks_changed`
  with `tasks:[{task_id, task_type, description, ambient?}]`, replace
  semantics. Useful if the daemon ever drives a session through the SDK;
  irrelevant to file observation.

### 1.3 Does `UserPromptSubmit` fire for a wake-up?

Binary evidence, 2.1.268: the hook input schema is
`{hook_event_name:"UserPromptSubmit", prompt, source?: "user"|"sdk"|"system"|"loop_wakeup"|"schedule_wakeup"|"poll_event"}`
with the description *"`loop_wakeup` = dynamic /loop wakeup, `schedule_wakeup`
= scheduled-task fire (CronCreate/routine), `system` = other machine-injected
turns (peer/channel messages, task notifications, auto-continuation)"*. The
submission-hook gate skips hooks only for `mode === "poll-event"` and the
empty poll dispatch (`skipSubmissionHooks`); `task-notification` mode is not
excluded. So: **yes, with `source: "system"`** for task notifications and
teammate messages, `loop_wakeup`/`schedule_wakeup` for timers.
`switchboard-ctl` already forwards `source` as `HookSource`. Not verified
live: the daemon's history for today's session records every idle→working
edge at wake-up times with rule `agent_graph_authority` (transcript), and
every working→idle edge the same way even though `stop_hook_summary` proves
the Stop hook ran, so the history log cannot discriminate. Measure in the
oracle phase (plan §7).

The `Stop` hook does fire at the end of the arming turn: between the
`end_turn` assistant line and the notification the transcript holds exactly
`system/stop_hook_summary` + `system/turn_duration`, then the queue lines of
§1.4 (46/46 in today's session, 122+45 across the corpus).

### 1.4 What is on disk between Stop and the wake-up

Nothing durable names an open wait. Checked:

| location | contents | verdict |
|---|---|---|
| transcript `queue-operation` lines (2,767) | `{type, operation: enqueue\|dequeue\|remove, content?, reason?}`; `enqueue.content` is the full `<task-notification>` text, `dequeue` has `content: null`, `remove` carries `reason: "absorbed_mid_turn"` (11) | the **earliest** wake signal: an `enqueue` of a task-notification with no later `dequeue`/`remove` = a wake is pending. Gap to the `user` line is sub-second when idle |
| `/tmp/claude-1000/<proj>/<sess>/tasks/<id>.output` | one file per shell/monitor/agent task; grows while running | proves the task existed; mtime is a liveness hint; no status, no registry, tmpfs |
| `~/.claude/tasks/session-*/N.json` | `{id, subject, description, status, blocks, blockedBy}` | the TaskCreate todo store, unrelated |
| `~/.claude/sessions/<pid>.json` | `{pid, sessionId, cwd, startedAt, procStart, version, peerFeatures:["notify_idle",…], kind, entrypoint, status:"shell", statusUpdatedAt, bridgeSessionId, name}` | `status` read `"shell"` while three monitors were armed; not a waiting flag |
| `~/.claude/daemon/roster.json`, `jobs/`, `teams/` | Claude's own background daemon roster, job scratch, team configs | no task state |
| crons / loop | ack says *"Session-only (not written to disk, dies when Claude exits)"*; loop stored in memory | none |
| Stop hook payload | `background_tasks[]`, `session_crons[]` | **the only authoritative record**, and only at the instant of Stop |

So the daemon has three sources, in decreasing authority: the Stop payload
(exact set at turn end), the transcript arm/close pairs (§1, continuous), and
the `.output` mtimes (liveness only).

### 1.5 Termination, per kind

| kind | ends by | on-disk record | never-closed arms in corpus |
|---|---|---|---|
| BackgroundBash | process exit → `status completed\|failed`; `TaskStop` → `killed`/`stopped`; session exit; harness reap at final response (`reapedAtFinalResponse` flag in the Bash result schema) | notification or TaskStop result; nothing on reap/exit | 91 of 166 (55%) |
| Monitor | `timeout_ms` elapsed → `<event>[Monitor timed out…]`; `TaskStop` → `[Monitor stopped]` + `status stopped`; output flood → `[Monitor stopped — too much output…]`; `persistent:true` → only TaskStop or session end | event line (housekeeping notifications are still `<task-notification>` lines) | 6 never notified + 11 events-only |
| Loop (ScheduleWakeup) | fires once per arm (re-armed by the model each wake); `{stop:true}`; harness age-out; session exit | ack for arm/stop; fire = `scheduled-trigger` user line | 0 fires observed |
| Cron | one-shot fires once; recurring until CronDelete/session exit | `scheduled_task_fire` system line + user line; CronDelete ack | 10 arms, 26 fires of one recurring job |
| Subagent (background) | agent stops with no live children → `status completed\|failed`; killed → `killed`; **may notify more than once** (binary note: *"the same task-id may notify more than once"* after a user resume) | notification | 145 of 179 (mostly teammates, which report via idle_notification instead) |
| Workflow | run ends → `completed\|failed`; `stopped` on resume/abort | notification with `<result>` | 5 of 110 |
| Teammate | `idle_notification` to the lead; `agents_killed` | teammate-message | — |

Monitor timeouts in the corpus: `timeout_ms` 3,600,000 (default in prompts)
and 1,800,000. Binary constants: 300000, 600000, 1800000, 3600000; remote
sessions clamp to 10/30 min and force `persistent:false`.

## 2. Measurements

### 2.1 Stop → wake-up (root idle the whole time)

Gap from the last `end_turn` assistant line to the notification, only where
the walk-back hits an `end_turn` (i.e. the session was orange-by-today's-rule
for the whole gap). Seconds.

| arm kind | n | p10 | p25 | p50 | p75 | p90 | p95 | max |
|---|---|---|---|---|---|---|---|---|
| Monitor event | 106 | 32 | 78 | 105 | 248 | 673 | 1,246 | 4,807 |
| Bash/Monitor terminal | 59 | 0.1 | 9 | 104 | 229 | 516 | 2,613 | 27,102 |
| Subagent | 41 | 0.3 | 92 | 311 | 592 | 1,375 | 1,759 | 27,157 |
| Workflow | 91 | 321 | 802 | 2,928 | 5,634 | 9,580 | 18,656 | 31,823 |

Arrival context: 297 of 318 notifications landed on an idle root
(after `end_turn`); 8 landed mid-turn (`stop_sequence`); the rest chained
directly after a previous notification turn. Consecutive notification-driven
turns before the next human prompt: 1 (30 sessions-runs), 2 (12), 3 (13),
5–7 (5), 18 (1), 32 (1). A monitor-driven session today ran 40 wake-ups over
2.4 h with two human prompts.

### 2.2 Arm → first/each notification

| arm kind | n | p25 | p50 | p75 | p90 | p95 | max |
|---|---|---|---|---|---|---|---|
| BackgroundBash | 57 | 146 | 210 | 392 | 888 | 2,654 | 11,559 |
| Subagent | 37 | 565 | 1,097 | 2,117 | 3,555 | 32,052 | 35,060 |
| Monitor (each event) | 112 | 242 | 1,278 | 4,822 | 6,998 | 8,779 | 37,800 |
| Workflow | 97 | 1,036 | 3,575 | 6,454 | 13,595 | 20,503 | 258,024 |

Monitor lifetimes (26 with ≥1 event): last event at p50 584 s, p75 1,312 s,
p90 9,436 s, max 37,800 s (10.5 h). Events per monitor p50 2, p95 7, max 49
(a persistent monitor, ended by TaskStop after 2.6 h).

### 2.3 What this says about a TTL

- A single cap does not fit: bash p95 is 44 min, monitors p95 2.4 h, workflows
  p95 5.7 h with a 71 h tail.
- Monitor events reset the clock: the p50 inter-event gap is 105 s and the
  p95 is 21 min, so "no event for 2× `timeout_ms`" is a safe stale rule and
  `timeout_ms` itself is the harness's own promise.
- The 55% of background Bash arms with no closing record are the real hazard:
  most are reaped at session exit or at the final response, silently. A
  waiting state for BackgroundBash must expire on its own.

## 3. Existing handling in the Go daemon

`rg` over `internal/` and `docs/`: `run_in_background` appears only as the
best-effort tag on fanout spawns (`fanout/observer.go:61,177-206`,
`transcript.go:233-258`, `history.go:102`); `<task-notification>` is known
only as "the thing that closes a background agent"
(`transcript.go:256`, `workflow-visibility.md:26,58,102`). No code reads
`Monitor`, `ScheduleWakeup`, `CronCreate`, `TaskStop`, `queue-operation`
lines, `background_tasks`, or `session_crons`. `docs/timing-hazards.md:37`
names the symptom ("teammate wakeup fires no `working` hook") from the
pre-agent-graph era. `docs/status-color-state-model.md` has no waiting state.

## 4. Design

### 4.1 Types

```rust
#[serde(rename_all = "snake_case")]
pub enum WaitKind { BackgroundBash, Monitor { persistent: bool }, Loop, Cron { recurring: bool }, Subagent, Workflow }

pub struct WaitId(String);                 // harness task id: b…, w…, a…, 8-hex cron id; loop = "loop"

pub struct OpenWait {
    pub kind: WaitKind,
    pub id: WaitId,
    pub call: Option<CallId>,               // tool_use_id of the arming call, when known
    pub armed_at: Timestamp,
    pub last_signal_at: Timestamp,          // arm, ack, monitor event, cron fire, Stop payload sighting
    pub expected_by: Option<Timestamp>,     // Monitor: armed + timeout_ms; Loop: scheduledFor; Cron: next fire; else None
    pub source: WaitSource,                 // Hook | Transcript | Both — for diagnostics and the shadow oracle
}

pub struct Waits { open: BTreeMap<WaitId, OpenWait>, generation: u64 }
```

`ClaudeRoot` gains `waits: Waits`. Codex has no equivalent today (no
background tool, no hook arrays); the field is Claude-only.

### 4.2 Sources and the edges that open/close a wait

Hook-first (exact, cheap):

| hook | effect on `waits` |
|---|---|
| `Stop{background_tasks, session_crons}` | **replace** the set: every listed task becomes/stays open with `last_signal_at = now`; any open wait absent from both arrays is closed (`closed_by_stop_payload`). Requires adding both arrays to `switchboard-ctl`'s `hookPayload` (ids, `type`, `status`, `schedule`, `recurring` only — drop `description`, `command`, `prompt` at the privacy boundary). |
| `SubagentStop{background_tasks}` | same replace rule for the child writer's own waits (a child that armed a monitor is itself waiting); root set untouched. |
| `UserPromptSubmit{source}` | `source ∈ {loop_wakeup, schedule_wakeup}` → close the matching Loop/one-shot Cron (recurring stays open, `last_signal_at = now`); `source == system` → no wait change (the transcript pair says which task fired); any source → root `active` as today. |
| `PostToolUse{tool_name ∈ Bash\|Monitor\|ScheduleWakeup\|CronCreate\|CronDelete\|TaskStop, tool_use_id}` | open/close a *provisional* wait keyed by `tool_use_id`; the transcript ack (next Observe) fills the task id. `TaskStop{task_id}` closes by id. |

Transcript-derived (the fallback, and what the oracle compares against):

| transcript evidence | edge |
|---|---|
| `tool_use` Bash with `run_in_background:true` + result text `Command running in background with ID: <b…>` (`toolUseResult.backgroundTaskId`) | open `BackgroundBash{id}` |
| `tool_use` Monitor + ack `Monitor started (task <b…>, …)` (`toolUseResult.{taskId, timeoutMs, persistent}`) | open `Monitor{id, expected_by = armed + timeoutMs}` |
| `tool_use` ScheduleWakeup without `stop` + ack `Next wakeup scheduled…` (`toolUseResult.scheduledFor`) | open/replace `Loop{expected_by = scheduledFor}`; `{stop:true}` closes it |
| `tool_use` CronCreate + ack (`toolUseResult.{id, recurring}`) | open `Cron{id}`; CronDelete ack `Cancelled job <id>` closes |
| `tool_use` Agent + ack starting `Async agent launched successfully` (`agentId:` line) | open `Subagent{id}` — this is the existing fanout "background" flag; `Spawned successfully` (teammates) is **not** a wait, the lead is not woken by them except via idle_notification |
| `tool_use` Workflow + ack `Workflow launched in background. Task ID: <w…>` | open `Workflow{id}` |
| `user` line, `origin.kind == task-notification`, `<task-id>X</task-id>` with `<status>` | close X (`completed/failed/killed/stopped`) |
| same, `<event>` only | `last_signal_at = now`; if the event text is `[Monitor timed out` or `[Monitor stopped` → close X |
| `user` line with `scheduledTaskId` / `origin.subkind == scheduled-trigger` | Loop → close; Cron one-shot → close; recurring → touch |
| `queue-operation{operation:enqueue, content:"<task-notification>…"}` | optional early signal: mark `wake_pending`; do not change status (the model is about to run; `UserPromptSubmit` arrives within a second when idle) |
| `system{subtype:agents_killed}` | close every `Subagent` wait |
| session rotation / process death | drop the set |

Both sources land on the same `Waits` map; the Stop payload wins on conflict
(it is the harness's own registry), and the daemon logs
`wait_set_mismatch{hook: n, transcript: m}` when they disagree, which is the
oracle check for this feature.

### 4.3 Reducer rule

Keep `Summary.status` and add one legacy value, `waiting` (green, distinct
CSS class so a user can tell "working" from "parked"). Fold, first match wins,
after the existing freshness gate:

1. `waiting_nodes > 0` → `permission`
2. root `active` → `working`
3. root `system_error` → grey
4. live child active/pending/running → `delegating`
5. root `idle` and `waits.live(now).len() > 0` → **`waiting`**
6. root `idle` → `idle`
7. else grey

`waits.live(now)` excludes expired entries (§4.5). Rule 5 sits below
`delegating` so a background subagent keeps today's colour and the oracle
diff is confined to the new kinds. `Summary` gains `open_waits: u32` and
`next_expected: Option<Timestamp>` (min `expected_by`) so the TUI can show
"waiting · 2 · next 09:41".

The `Since` rule is unchanged: `waiting` is its own status value, so entering
it re-anchors, and a monitor event that flips waiting→working→waiting in two
seconds produces two transitions — the history log wants exactly that.

### 4.4 Interaction with the hooks the plan already wires

- `Stop` is now a two-message edge: `runtime = idle` **and** `waits =
  payload`. With the arrays present, rule 5 evaluates in the same `apply`
  and the chip never shows orange between Stop and the next wake-up.
- `UserPromptSubmit` on a wake-up: `runtime = active` as today; retained
  terminal children cleared as today; the wait closes from the transcript
  pair or the `source`, not from the prompt itself. Do not clear `waits` on
  `UserPromptSubmit`: a monitor stays armed across the wake it caused.
- `SubagentStop` with a non-empty `background_tasks` means the child is
  parked, not finished: keep its fanout lifecycle `running` (the binary's
  own phrase is "agent finished; parked on pending background work") and
  give the child node its own `waits`. The 30-minute stale cap for children
  must be suspended while the child's wait set is non-empty and unexpired.
- `PermissionRequest` unaffected: red still beats everything.
- `SessionEnd{reason}` drops the set.
- Hydrate/Restore: persist `waits` in `state.json`; on restart re-read from
  the transcript tail (the arm acks are in the last few hundred lines) and
  trust the next Stop payload to correct it.

### 4.5 Failure modes and caps

A wait whose notification never comes is the normal case for background
Bash (55%) and the persistent-monitor case by construction. Rules:

| kind | `expected_by` | stale cap (close silently, log `wait_expired`) | rationale |
|---|---|---|---|
| BackgroundBash | none | `max(2 × input.timeout, 20 min)`, hard 1 h | p95 44 min; reaped-on-exit tasks never notify |
| Monitor, `timeout_ms` | `armed + timeout_ms` | `expected_by + 5 min` (a timed-out monitor emits its own event) | harness promise; events reset `last_signal_at`, not the cap |
| Monitor, `persistent` | none | `last_signal_at + 2 h`, hard 12 h | p90 event gap 11 min; the 10.5 h outlier is real |
| Loop | `scheduledFor` (clamped ≤ harness max, 1 h) | `expected_by + 2 min` | fires exactly then or was cancelled |
| Cron one-shot | next match of `schedule` | `expected_by + 2 min` | same |
| Cron recurring | next match | none while the process lives; `CronDelete` or Stop payload absence closes | fires forever |
| Subagent | none | existing 30 min quiescence on its transcript, already implemented | unchanged |
| Workflow | none | `max(run dir mtime + 90 s quiet, 12 h)` | p95 5.7 h; run dir quiescence is already observed (survey 03 §2.5) |

Expired → the wait leaves `live()`, the fold falls through to `idle`, orange.
The Stop payload re-opens it on the next turn end if the harness still lists
it, so the cap is a display decision, not data loss. Other hazards:

- **Ack without task id** (3 malformed Monitor calls, 7 denied Workflows,
  errors): open nothing; a `tool_use` with an error result is not an arm.
- **Same task id notifying twice** (resumed subagents): closing an unknown
  id is a no-op; a second `completed` after close is logged, not applied.
- **Mid-turn absorption** (`queue-operation remove/absorbed_mid_turn`,
  `stop_sequence` arrivals): the close edge still comes from the `user`
  line's `<task-id>`; nothing special.
- **Cross-writer ids**: a child's monitor (`isSidechain` file) must not open
  a root wait; key `Waits` per writer like `pending`.
- **Hook arrays missing** (older harness, `switchboard-ctl` not rebuilt):
  transcript source alone; caps still apply. Treat an absent array as "not
  reported", not as empty — only an explicitly empty array closes waits.
- **Teammates**: `Spawned successfully` never opens a wait; a lead blocked on
  teammates renders `delegating` through the fanout path as today.

### 4.6 Acceptance fixtures (add to survey 09's set)

1. Bash bg arm → Stop → 210 s → `completed` notification → `UserPromptSubmit`
   → working. Expect waiting for 210 s, no orange.
2. Monitor `timeout_ms = 3600000` with 4 events then `[Monitor timed out`.
   Expect waiting between events, idle after the timeout event's turn.
3. Persistent monitor, 49 events, TaskStop. Expect waiting throughout, idle
   after the TaskStop turn.
4. Bash bg arm, no notification, session exit at +34,861 s. Expect waiting
   until the 1 h cap, then idle, then gone.
5. Stop payload `background_tasks: []` while the transcript still shows an
   open bash arm. Expect closed (hook wins), `wait_set_mismatch` logged.
6. Cron recurring `*/20`, 26 fires. Expect waiting continuously, `working`
   during each fire's turn, never orange.
7. Subagent `SubagentStop` with non-empty `background_tasks`. Expect child
   stays live, parent `delegating`.

## 5. Open questions for the plan

- Is `waiting` a new `Status` variant or `Delegating` with a `kind` tag? The
  fold above keeps them distinct; the bar needs one more colour class either
  way.
- Whether to persist `expected_by` for cron via a real cron parser (a small
  dependency) or treat recurring crons as uncapped and one-shots as
  transcript-only.
- Confirm live in the oracle phase: (a) `UserPromptSubmit.source` on a
  task-notification is `system`; (b) `Stop.background_tasks` lists a
  persistent monitor; (c) whether `Stop` fires at all when the harness reaps
  a background Bash at the final response.
