# 11 — Workflows, subagents and agent teams: what is on disk, what the hooks add, and how the Rust daemon should model children

Read-only survey, 2026-09-11, against `~/.claude` on this machine (Claude Code
2.1.268) and the Go daemon at `~/Projects/switchboard`. Every count below is a
`fd`/`jq` census over `~/.claude/projects`; string values are trimmed, no prompt
text is quoted. Companion to survey 03 §2.5–2.6 (the Go fanout lifecycle) and
the plan's §4 `HookKind`.

Headline: the Go daemon reconstructs "is this session delegating" from three
file scans bounded by two timers. Claude Code 2.1.268 now writes four
structural records the Go daemon never reads — `toolUseResult` on the parent's
launch-ack line, `origin.kind` on the wake-up line, `background_tasks` on every
`Stop`/`SubagentStop` hook, and the `TeammateIdle`/`TaskCreated`/`TaskCompleted`
hooks — and together they answer the delegating question directly. The scans
stay as the restart backfill and the display source; they stop being the
authority.

---

## 1. On-disk census

Under `~/.claude/projects/<slug>/<session-id>/` (the sibling dir of
`<session-id>.jsonl`):

| artifact | count | note |
|---|---|---|
| `subagents/` | 42 sessions | flat dir: `agent-<id>.jsonl` + `agent-<id>.meta.json` |
| `subagents/workflows/` | 28 sessions | one `wf_<8hex>-<3hex>/` per run |
| `wf_*` run dirs | 81 | each: `journal.jsonl` + agent jsonl/meta pairs (1716 pairs total) |
| `workflows/scripts/` | 23 dirs | `<name>-wf_<8hex>-<3hex>.js`; one file has no run suffix |
| `tool-results/` | 49 sessions | `<id>.txt` (288), `page-NN.jpg` (138), `.pdf` (13): persisted large tool outputs |
| `custom-title.json` | 37 | `{"customTitle":"…"}` (`/rename`) |
| `memory/` | 24 (project level) | `MEMORY.md` + per-fact files; not per session |
| `agent-*.meta.json` | 1922 | 1765 hex-stem, **158 named-stem** (teammates, §2) |
| `agent-*.jsonl` | 1924 | two orphan jsonl without meta |

Note `fd` silently honours `~/.claude/.gitignore`, which lists `projects/`,
`tasks/`, `teams/` — a census run without `-I` returns zero. A Rust walker must
not use gitignore-aware crates for this tree.

Same session id, two slugs: `-Projects-Arachne/44ed8caa…/subagents/workflows/`
holds the runs while `-Projects-Arachne-ops/44ed8caa…/workflows/scripts/` holds
the scripts (the session's cwd moved). The Go name lookup
(`scriptNamesByRunID`, sibling-of-transcript only) yields "" for such runs. The
same run id can also appear under two session dirs with different contents
(`wf_931b300c-fd4`: 69 files in session 4877eaf1, 5 in 534e47b7, a fork that
resumed it). Run identity is `(session, run_id)`, never `run_id` alone.

State outside `projects/`:

- `~/.claude/teams/<team>/config.json` — 14 teams; 12 named `session-<8hex>`.
  Shape: `{name, description?, createdAt, leadAgentId:"team-lead@<team>",
  leadSessionId, members:[{agentId:"<name>@<team>", name, agentType, model,
  prompt, color, planModeRequired, joinedAt, tmuxPaneId:"leader"|"in-process",
  cwd, subscriptions:[], backendType:"in-process"}]}`. **Members are never
  pruned**: `session-05624f09` still lists its eight finished survey
  teammates. `inboxes/team-lead.json` exists for two teams and is `[]` (drained
  on delivery). The team name is *not* the transcript session id: team
  `session-1f0af110` has no `1f0af110*.jsonl` anywhere; its teammates' metas
  sit under `05624f09…/subagents/`. `leadSessionId` cannot be joined to a root.
- `~/.claude/tasks/<team>/` — `N.json` (11 total across 8 non-empty dirs),
  `.lock`, `.highwatermark` (max id). Task: `{id, subject, description, status,
  blocks:[], blockedBy:[], activeForm?}`; observed `status` ∈ {pending,
  completed}; no owner field seen. Most dirs are empty (`TaskCreate` unused).
- `~/.claude/sessions/<pid>.json` — live peer registry, one per running CLI:
  `{pid, sessionId, cwd, startedAt, procStart, version, peerProtocol,
  peerFeatures, kind:"interactive", entrypoint:"cli", pidDomain,
  messagingSocketPath, name, nameSource:"user", status, statusUpdatedAt,
  bridgeSessionId}`; observed `status` ∈ {busy, shell}. `pid + procStart` is
  exactly the plan's `ProcId`; this file is a second discovery source with the
  session id and cwd attached, and `status` is Claude's own idle/busy word.
- `~/.claude/daemon/roster.json` + `~/.claude/jobs/<short>/{state.json,
  timeline.jsonl}` — Claude Code's own background-session supervisor
  (`cc-daemon`). `state.json`: `{state:"working"|…, tempo, inFlight:{tasks,
  queued, kinds:["monitor"]}, fan:[{id, kind, label, startedAt}], sessionId,
  cwd, backend:"daemon", template:"bg", cliVersion}`. These are headless roots
  the process scanner already sees; the file adds their self-reported state.
- `/tmp/claude-1000/<slug>/<session>/tasks/<id>.output` — background task
  output, tmpfs, volatile. Not load-bearing.

## 2. `agent-*.meta.json` field census (1922 files)

| key | files | values seen |
|---|---|---|
| `agentType` | 1922 | `workflow-subagent` 1509; `general-purpose`, `Explore`, `Plan`, `reviewer`, `researcher`, `implementer`, `fixer`, `claude-code-guide`, `fork`; **for teammates it is the teammate name** (`inv-workflows`, `angle-A`, …) |
| `spawnDepth` | 1922 | **0 or 1 only**; 0 ⇔ teammate spawned by the lead; 1 for everything else, including 30 teammates and 3 Explores that carry `parentAgentId`. No depth 2 in the corpus. |
| `description` | 400 | short label |
| `requestShape` | 212 | `foreground` 195 (workflow agents with a phase), `background` 17 |
| `requestNonInteractive` | 212 | `true` ⇔ `requestShape:"background"` |
| `workflowPhase` | 195 | `Implement` 66, `Collect` 34, `Verify` 34, `Review` 25, `Research` 13, `Qualify` 12, `Compute`, `Sync`, `Reclean`, `Prepare`, `Ledger`; only under `wf_*` |
| `name` | 190 | teammate name (158) or plain-agent `name:` (32) |
| `model` | 172 | `opus`, `sonnet`, `haiku`, `fable`, `Fable`, `opus[1m]`, `claude-opus-5`, `claude-fable-5-1`, `claude-fable-5[1m]` — free text |
| `taskKind` | 157 | **only `in_process_teammate`** |
| `teamName` | 157 | `session-<8hex>` or a user-chosen name |
| `permissionMode` | 157 | `auto` (teammates) |
| `color` | 157 | blue green yellow purple orange red cyan pink |
| `spawnedWithWorktree`/`worktreePath` | 74 | plus `worktreeBranch` 31 |
| `customAgentType` | 54 | the *real* type of a teammate (`reviewer`, `general-purpose`) when `agentType` holds its name |
| `toolUseId` | 47 | plain agents only; absent on every teammate and workflow agent |
| `parentAgentId` | 33 | bare id of a **sibling** file in the same flat dir (33/33 resolve) |
| `stoppedByUser` | 1 | `true` on an Explore |
| `isFork` | 1 | `true`, `agentType:"fork"` |

Rules that the census supports (each verified on all 1922 files):

- **(a) plain subagent**: no `taskKind`; stem `agent-a<16hex>`; `agentType` is
  a type; `toolUseId` usually present. Example (trimmed):
  `{"agentType":"Explore","description":"…","toolUseId":"toolu_01…","spawnDepth":1}`.
- **(b) workflow agent**: path contains `/subagents/workflows/wf_`. That is the
  only universal test — `agentType` is `workflow-subagent` in 1509 but a custom
  type (`implementer`, `reviewer`, …) in the rest. Newer runs add
  `workflowPhase` + `requestShape:"foreground"`. Never `toolUseId`.
- **(c) teammate**: `taskKind == "in_process_teammate"` (158/158 also have a
  `name` and a **named stem `agent-a<name>-<16hex>`**; 157 have `teamName`).
  Example: `{"agentType":"inv-workflows","name":"inv-workflows","spawnDepth":0,
  "requestShape":"background","requestNonInteractive":true,"model":"claude-fable-5-1",
  "taskKind":"in_process_teammate","teamName":"session-1f0af110","color":"yellow",
  "planModeRequired":false,"permissionMode":"auto"}`. A teammate has two ids:
  the file stem (`ainv-workflows-f9fd…`, what hooks and jsonl `agentId` carry)
  and `<name>@<team>` (what SendMessage, the launch ack and `config.json` use).
- **(d) background vs foreground**: `requestShape` when present (17
  background). It is absent on 1710 files, so the parent transcript's
  structured launch record (§5) is the reliable source: `isAsync:true` on 38 of
  39 plain spawns, the one exception `status:"forked"`.

`spawnDepth ≥ 2` no longer occurs; nesting is `parentAgentId`. The Go rule
"don't count depth ≥ 2" (`fanout/observer.go:276`) is dead on this corpus, and
the live nesting fact (a teammate spawned by a review coordinator has
`spawnDepth:1` + `parentAgentId`) is one the Go daemon ignores.

## 3. Workflow journals and run lifecycle

`wf_*/journal.jsonl` line types over all 81 runs (3395 lines):

| type | lines | keys | meaning |
|---|---|---|---|
| `launched` | 9 | `type` only | first line of a run (2026-09 format); no agent id |
| `started` | 1716 | `type,key,agentId` (+`phase,label` on 195 new-format lines) | an `agent()` call began |
| `result` | 1586 | `type,key,agentId,result` | its structured result landed (payload arbitrary, up to 280 KB per journal) |
| `failed` | 84 | `type,key,agentId` | **terminal**: `started X` → `failed X`, same key, never a `result` |

`key` is `v2:<sha256>` — the memoisation key that `resumeFromRunId` replays
("completed agents return cached results" per the launch ack). Journals carry
**no timestamps**; timing is file mtime. First→last line per run: 63
`started→result`, 8 `launched→result`, 5 `started→failed`, 1
`launched→failed`, 4 `started→started` (killed, orphaned forever).

The Go scanner parses only `started`/`result` (`transcript/workflows.go:150`).
A `failed` agent therefore stays in flight until its transcript mtime passes
the 30-minute stale cap — 84 such lines across 20 runs. That is a live defect
to fix in the port, not a design choice to carry.

Lifecycle from the files alone, run `wf_77088cf3-473` (session c7c15cd6, times
UTC): `Workflow` tool_use in the parent at 01:54:50.392 → first two
`agent-*.meta.json` at 01:54:52.9 and 01:54:55.9 → first agent jsonl mtime
01:57:43 → run dir mtime 02:15:49 (last file created) → journal last write
02:20:11.404 → parent `<task-notification>` user line at **02:20:11.428**, 24 ms
later, with `<tool-use-id>` equal to the launching tool_use. The first run in
that session shows the same 90 ms gap (22:29:26.510 → .599). The run's end is
therefore observable exactly, from the parent transcript, paired by
`tool_use_id` — the Go daemon's 90-second quiet grace exists only because it
never pairs the notification.

Liveness bounds today (`fanout/observer.go:44-56, 339-392`): a run is active
while any `started − resulted − stale-closed > 0`, or its journal (or, absent
a journal, its dir) mtime is within 90 s; an agent is stale-closed when its
jsonl mtime is older than 30 min. Both timers are replaceable: the run-end by
the notification, the agent-end by `result`/`failed` plus `SubagentStop`.

## 4. Agent teams as they manifest

- **Spawn.** The lead's `Agent` tool_use with `name` set; the tool_result text
  starts `Spawned successfully. (This tool result is internal metadata — …` and
  the same user line carries the structured record
  `toolUseResult: {status:"teammate_spawned", teammate_id:"<name>@<team>",
  agent_id:"<name>@<team>", agent_type, model, name, color, tmux_session_name,
  tmux_window_name, tmux_pane_id:"in-process", team_name, is_splitpane,
  plan_mode_required, prompt}`. Meta as in §2(c). `config.json` gains a member.
- **Teammate transcript.** `agent-a<name>-<hex>.jsonl`; every line has
  `agentId:"<a name-hex>"`, `isSidechain:true`, `sessionId` = the **root's**
  transcript id. Lead messages arrive as user lines whose text opens
  `<teammate-message teammate_id="team-lead" summary="…">`, with no `origin`.
- **Teammate → lead.** `SendMessage` input `{to, summary, message,
  type:"message", recipient, content}` (legacy duplicates). In the lead it is a
  user line opening `<teammate-message teammate_id="<name>" color="<c>"
  summary="…">`, also without `origin`.
- **Idle.** When a teammate stops, its jsonl ends with an assistant `end_turn`
  — the exact marker the Go scanner reads as *completed*
  (`subagentTerminalReason`). Of 262 named-stem transcripts, 179 end
  `end_turn`, 52 `stop_sequence`, 21 assistant without stop, 5 `tool_use`,
  554 end on a user line. At the same moment the lead receives a user line
  `origin:{kind:"task-notification"}, promptSource:"system"` whose text is
  `<task-notification><task-id/><tool-use-id/><output-file/><status>completed
  </status><summary/>` and (for teammates) a `<note>` saying the notification
  fires *each time this agent stops with no live background children of its
  own* and that it can still be messaged. Corpus `<status>` values: completed
  1573, failed 245, killed 39, stopped 36, running 3.
- **Shutdown.** No on-disk marker. A teammate that received a shutdown message
  at 05:45 wrote two `attachment` lines and a final `end_turn` at 05:52 —
  indistinguishable from idle. The hook `SubagentStop{agent_id}` is the only
  terminal edge for a teammate. `config.json` membership never changes.
- **Hooks (binary-verified, 2.1.268).** `TeammateIdle{teammate_name,
  team_name}` ("when a teammate is about to go idle"; may block);
  `TaskCreated`/`TaskCompleted{task_id, task_subject, task_description?,
  teammate_name?, team_name?}` (`TaskCompleted` may block); `team_name` is
  documented deprecated on all three. `SubagentStart{agent_id, agent_type}`;
  `SubagentStop{stop_hook_active, agent_id, agent_transcript_path, agent_type,
  last_assistant_message?, background_tasks?, session_crons?}`; `Stop` carries
  the same `background_tasks`: an array of `{id, type:"shell"|"subagent"|
  "monitor"|"workflow", status, description, command?}` described in the
  schema as letting hooks "distinguish 'session is done' from 'session is
  paused waiting for background work to wake it'". Empty when nothing is in
  flight.

So: the daemon can list a team's members (metas with `teamName`, or
`config.json`), can tell who is idle (`TeammateIdle` by name; or the
`task-notification` line in the lead; or the teammate's `end_turn` + lead
still alive), and can tell whether the lead is waiting on them (the lead's
`Stop` with a non-empty `background_tasks`, or the lead idle with teammates
whose last line is not `end_turn`). What it cannot tell from disk is *ended*
versus *idle* for a teammate; only `SubagentStop` says that.

## 5. Parent-transcript signals

- **`Agent` tool_use** (179 across all parent transcripts). Input keys seen:
  `subagent_type, name, description, prompt`; schema also allows
  `run_in_background, isolation, model, mode, team_name`. `run_in_background`
  is never true on an `Agent` spawn in the corpus (Go doc
  `async-agent-launch-ack.md` measured 0/69; still true).
- **Launch ack** (tool_result text, both wordings live): `Spawned
  successfully. (This tool result is internal metadata — …` and `Async agent
  launched successfully. (This tool result is internal metadata — …`, followed
  by `agentId:`/`agent_id:` lines. The **same user line** carries
  `toolUseResult: {status:"async_launched", agentId, isAsync:true,
  canReadOutputFile:true, outputFile, description, prompt, resolvedModel}` (38
  of 39 plain spawns; the other is `status:"forked"`). Teammates:
  `status:"teammate_spawned"` (§4). This structured field is what should
  classify a spawn, not a prefix match on the prose.
- **`Workflow` tool_use** (26 launches in two sessions). Input keys:
  `{scriptPath,args}` 19, `{scriptPath,args,resumeFromRunId}` 4, `{script}` 2,
  `{name,args}` 1, `{scriptPath}` 1. Ack text: `Workflow launched in
  background. Task ID: <9 alnum>\nSummary: …\nTranscript dir: …/wf_<run>\nScript
  file: …\nRun ID: wf_<run>`. Structured: `toolUseResult: {status:
  "async_launched", taskId, taskType:"local_workflow", workflowName, runId,
  summary, transcriptDir, scriptPath}` — the run id, name and dir, at launch,
  with the tool_use id. The Go daemon reads none of it.
- **Wake-up lines.** A user line with `origin:{kind:"task-notification"}` and
  `promptSource:"system"` (341 in the corpus; other `origin.kind`: `human` 850,
  `auto-continuation` 3; other `promptSource`: `typed`, `sdk`, `queued`,
  `suggestion_accepted`). Text: `<task-notification>` block as in §4, with
  `<tool-use-id>` pairing it to the launching tool_use for both agents and
  workflows; `<task-type>` appears only on artifact watches. This is the
  structural way to tell a background completion from a human prompt — the
  Go daemon's `UserPromptSubmit`-drops-the-cohort rule (`observer.go:511`)
  should key on `origin.kind == "human"`, if UserPromptSubmit fires at all on a
  system wake (unmeasured).
- **Task tools.** `TaskCreate{subject, description, activeForm}`,
  `TaskUpdate{taskId, status}` (159) / `{taskId, addBlockedBy}`, `TaskList{}`,
  `TaskGet{taskId}`. Only 13 `TaskCreate` in the corpus; teams mostly do not
  use the shared task list.
- **Cross-session peers.** A user line `Another Claude session sent a message:
  \n<agent-message from="<name>">` — delivery over
  `sessions/<pid>.json.messagingSocketPath`, not a team. Not a child.

## 6. Invariants the Go tests pin

`fanout/observer_test.go`, `observer_workflow_test.go`, `snapshot_test.go`,
`transcript/subagents_test.go`, `workflows_test.go`:

1. Spawn and stop are emitted **exactly once** per agent id; a second reconcile
   with no change emits nothing and holds the count (`TestReconcile_spawnOnceCompletionAndInflight`).
2. The spawn set is the union of meta and jsonl stems; meta-only ⇒ `pending`,
   jsonl present ⇒ `running`; a non-JSON meta is tolerated; mtime prefers the
   jsonl (`TestSubagentsForTranscript`, `TestSubagentsModTimePrefersJSONL`,
   `TestObserveDistinguishesPendingAndStaleInterruptedChildren`).
3. A teammate with no `toolUseId` counts (`TestReconcile_teammateCountsWithoutToolUseID`);
   depth ≥ 2 is emitted but not counted (`TestReconcile_grandchildNotCountedButEmitted`).
4. Restart/`--resume` never re-emits: the seen-set is seeded from the history
   log, the cursor primed to EOF (`TestReconcile_seedsFromHistoryNoReemitOnRestart`,
   `TestReconcile_shouldNotReannounceWorkflowAfterDaemonRestart`).
5. A launch ack (either wording, no `run_in_background` in the input) never
   completes a child and tags the spawn `Background`; completion is jsonl
   `end_turn` or the stale cap (`TestReconcile_shouldStayInFlightWhenTheAgentSpawnCarriesNoBackgroundFlag`,
   `TestObserveKeepsAsyncLaunchAcknowledgementRunning`,
   `TestTasksSinceShouldFlagLaunchAcksWhenTheResultOnlyAcknowledgesTheSpawn`).
6. Stale cap force-closes as `interrupted` with reason `stale_quiescent`,
   carried onto the stop event (`TestReconcile_staleCapForceClosesLeak`,
   `TestReconcile_shouldForceCloseKilledRunsOrphansAfterStaleCap`).
7. Parent cursor: a shrunken file re-reads from 0 and idempotence lives in the
   id seen-set (`TestObserveResetsParentCursorAfterTruncation`,
   `TestTasksSinceOffsetThreading`).
8. Workflow: `result` (never jsonl) completes a workflow agent; a fresh journal
   with everything resulted is *between waves*, not done; drained + quiet past
   the grace ⇒ one `workflow_stop`; run name from the script filename split on
   the last `-wf_` (`TestReconcile_shouldBridgeBetweenWavesWhileJournalIsFresh`,
   `…shouldStopRunWhenDrainedAndJournalGoesQuiet`, `…ShouldSplitScriptNameOnLastRunIDBoundary`).
9. Path safety: a hook-supplied `agent_id` with a separator yields no path; no
   second `agent-` prefix strip; a trailing `.jsonl` is tolerated
   (`TestSubagentPath`, `TestSubagentMetaPath`, `TestWorkflowAgentTranscriptShouldRejectPathEscapes`).
10. Display name: `name` → `agentType` → "" (`TestSubagentDisplayName`).

Keep 1, 2, 4, 5, 7, 9, 10 verbatim as acceptance tests. Revise 3 (nesting is
`parentAgentId`, not depth), 6 (the cap becomes a last resort behind
`failed`/`SubagentStop`), and 8 (the notification ends a run; `failed` ends an
agent).

---

## 7. Proposed detection design for the Rust daemon

### 7.1 Authority per fact

| fact | authority | backfill / cross-check | redundant now |
|---|---|---|---|
| child exists, kind, ids | `agent-*.meta.json` (stem + `taskKind` + path) | `SubagentStart{agent_id, agent_type}` as the invalidation trigger | — |
| plain spawn is async / forked | parent line `toolUseResult.status` (`async_launched`/`forked`/`teammate_spawned`) | ack prefix match | prose prefix match |
| plain child finished | `SubagentStop{agent_id}` | own jsonl `end_turn` (non-teammate only); stale cap | parent `tool_result` pairing (never a result for async spawns) |
| teammate idle | `TeammateIdle{teammate_name}` | lead's `origin.kind=task-notification` line; teammate jsonl `end_turn` | — |
| teammate ended | `SubagentStop{agent_id}` only | none on disk; lead exit ends all | — |
| workflow run started, id, name, dir | parent `toolUseResult{runId, workflowName, transcriptDir, taskId}` | `wf_*` dir appearing; script filename | script-name parsing, 90 s dir-mtime rule |
| workflow agent started / done | journal `started` / `result` **or `failed`** | agent jsonl mtime; `SubagentStop` if it fires for workflow agents (unmeasured, Go doc says it did not in 2.1.1xx) | — |
| workflow run ended | lead's `task-notification` with matching `<tool-use-id>` and `<status>` | journal quiet grace as fallback only | 90 s quiet grace as primary |
| root waiting on children | `Stop.background_tasks` non-empty | live child count from the scans | in-flight count as the *decision* input |
| root's new human turn | `UserPromptSubmit` + line `origin.kind == "human"` | — | treating every UserPromptSubmit as a cohort reset |

### 7.2 Types

```rust
pub struct AgentId(String);                       // bare file stem: "a3f…" or "ainv-workflows-f9fd…"
pub struct TeamAddr { name: String, team: String } // "<name>@<team>", messaging identity only

pub enum ChildKind {
    Subagent  { tool_use_id: Option<CallId>, launch: Launch },
    WorkflowAgent { run: RunKey, phase: Option<String>, label: Option<String> },
    Teammate  { team: String, name: String, addr: TeamAddr, real_type: Option<String>, color: Option<String> },
}
pub enum Launch { Unknown, Async, Forked, Foreground }

pub struct Child {
    pub id: AgentId, pub parent: Option<AgentId>,   // parentAgentId; None == root
    pub kind: ChildKind, pub agent_type: String, pub description: Option<String>,
    pub lifecycle: Lifecycle,                       // reuse agentgraph::Lifecycle
    pub evidence: Evidence,                         // which source set lifecycle; for diagnostics only
    pub spawned_at: Timestamp, pub updated_at: Timestamp, pub ended_at: Option<Timestamp>,
    pub transcript: PathBuf,                        // never on the wire
}
pub struct RunKey { session: SessionId, run_id: String }   // wf_ ids collide across sessions
pub struct WorkflowRun {
    pub key: RunKey, pub name: Option<String>, pub tool_use_id: Option<CallId>, pub task_id: Option<String>,
    pub started: BTreeSet<AgentId>, pub resulted: BTreeSet<AgentId>, pub failed: BTreeSet<AgentId>,
    pub journal_offset: u64, pub state: RunState,   // Launched | Active | Ended{status} | Orphaned
}
pub struct Team { pub name: String, pub members: BTreeMap<AgentId, TeamAddr>, pub idle: BTreeSet<AgentId> }
pub struct Children { pub by_id: BTreeMap<AgentId, Child>, pub runs: BTreeMap<RunKey, WorkflowRun>,
                      pub team: Option<Team>, pub root_background: Option<Vec<BackgroundTask>> }
```

`Evidence` is an enum {Hook, Journal, Transcript, Notification, Mtime} so a
stale-cap close is visibly weaker than a hook close, replacing the Go
`SuspectReason` string with something the reducer can rank.

### 7.3 Lifecycle derivation

Subagent:
- meta or jsonl appears → `Pending`; jsonl appears → `Running`. `SubagentStart`
  is an `Invalidate(root)` trigger, never a counter write (Go rule kept).
- `SubagentStop{agent_id}` → `Completed` (`Evidence::Hook`). Else jsonl last
  complete line `stop_reason == "end_turn"` → `Completed`
  (`Evidence::Transcript`) — **only for non-teammates**. Else mtime older than
  the stale cap → `Interrupted` (`Evidence::Mtime`).
- A launch ack is never completion; `Launch` comes from `toolUseResult.status`.
- `stoppedByUser:true` in the meta → `Interrupted` immediately.

Workflow agent:
- journal `started` → `Running` (`Pending` until its jsonl exists);
  `result` → `Completed`; **`failed` → `Errored`**; stale cap → `Interrupted`.

Workflow run:
- `Workflow` tool_use answered by `toolUseResult{runId,…}` → `Launched`,
  keyed by `(session, runId)`, name and dir known at once.
- first journal line or any agent `Running` → `Active`.
- lead's user line with `origin.kind == "task-notification"` whose
  `<tool-use-id>` matches → `Ended{status}` (`completed|failed|killed|stopped`),
  and every still-open agent of that run → `Interrupted` at that instant (no
  30-minute wait).
- fallback when the notification is missed (compaction, daemon restart): no
  agent running and journal quiet past 90 s → `Ended{unknown}`; agent stale
  cap unchanged.

Teammate:
- meta with `taskKind` → `Running` from spawn (teammates start immediately).
- `TeammateIdle{teammate_name}` → `Idle` — a **new** lifecycle value, or
  `Running` with `attention: None` and an `idle: true` flag on `Team`; the
  transcript `end_turn` is the backfill for the same fact, never a completion.
- `SubagentStop{agent_id}` → `Completed`. Root death → every teammate `Shutdown`.
- Nesting: `parentAgentId` sets `parent`; children of a child render nested and
  are excluded from the *root's* delegating decision only if their parent is
  itself terminal (a coordinator waiting on its verifiers is still delegating).

### 7.4 Caps

Keep two timers, both as last resort: `stale_cap = 30 min` on a child's own
transcript mtime, `quiet_grace = 90 s` on a run journal. Remove the run-dir
mtime rule (superseded by the launch record). Add one: a `Pending` child (meta
only, no jsonl) older than `stale_cap` → `Interrupted`, which the Go code
never closes.

### 7.5 What "delegating" means

The reducer rule stays "root idle ∧ live child ⇒ delegating", with `live`
sharpened:

- **Working team**: any teammate `Running` and not idle, or any subagent /
  workflow agent `Running`/`Pending` → delegating. The root's own
  `Stop.background_tasks` (non-empty) says the same thing at hook speed and
  should win on conflict (`Evidence::Hook` outranks `Mtime`).
- **Idle team**: every teammate idle (`TeammateIdle` seen after its last
  `Running` evidence, or transcript `end_turn` newer than the last lead
  message to it) and no other live child → the root is **idle, not
  delegating**, even though `config.json` and the metas say the team exists.
  This is the case the Go daemon renders green today (a parked team of eight
  `end_turn` teammates counts as eight in-flight until the stale cap).
- **Lead waiting**: root idle, a teammate mid-turn → delegating; when the
  teammate stops, the root wakes on the task-notification line, so the
  transition to working is observed rather than inferred.
- Grey cases: `Ended{killed|stopped|failed}` runs with orphans → orphans
  `Interrupted`, no delegating credit.

### 7.6 What the new hooks make redundant

- `SubagentStop` with `agent_id` → the jsonl-tail `end_turn` read for plain
  subagents, and the whole parent `tool_result` pairing (`TasksSince`
  `resultDone`, `launchAckPrefixes`, the `Background` dead-flag lineage).
- `Stop.background_tasks` → the in-flight *count* as the input to the
  delegating decision at turn end; the scans remain for display and for the
  restart backfill only.
- `TeammateIdle` → the only way to see an idle team as idle; nothing on disk
  distinguishes it from a working one except the end_turn heuristic.
- `toolUseResult` on the launch line → script-filename name parsing, ack-prefix
  matching, `run_in_background`, and the 90 s run-dir-mtime "launching" rule.
- `origin.kind` on wake lines → the ambiguity of `UserPromptSubmit` as a cohort
  reset.

Still to measure live before relying on them: does `SubagentStop` fire for
workflow agents and for teammates on shutdown; does `TeammateIdle` fire for a
teammate that stops on an error; does `UserPromptSubmit` fire on a system
wake; is the hook `agent_id` for a teammate the file stem or `<name>@<team>`
(the binary derives `agent_transcript_path` from it, so the stem is likely).
