# Survey 03 — Provider observation and the status state machine

Read-only survey of `~/Projects/switchboard` (Go) for the Rust rewrite. Scope:
`internal/provider/{provider.go,claude,codex}`, `internal/transcript`,
`internal/agentgraph`, `internal/fanout`, `cmd/switchboard/agent_observation.go`,
`cmd/switchboard/codex_hook_transitions.go`, the hook boundary in
`internal/rpc/rpc.go` and `cmd/switchboard-ctl/main.go`, plus the legacy
reconciler in `cmd/switchboard/main.go` that still runs for graph-less sessions.
All paths below are relative to `~/Projects/switchboard/`.

Line counts for scale: claude observer 1670, codex observer 1454, codex protocol
948, transcript 1269, subagents 451, fanout observer 542 + snapshot 289,
coordinator 1307, codex hook transitions 1383. Roughly 9,300 lines of
non-test Go, backed by about 7,500 lines of design docs.

---

## 1. Responsibilities

- **Derive one chip colour per root agent process** (Claude Code or Codex TUI)
  from partial, lossy, asynchronous evidence: hook edges, transcript files on
  disk, a Codex JSON-RPC stream, process death, and timers.
- **Own the provider-neutral graph** (`internal/agentgraph`): a root node plus
  explicitly parented child agents, each with three independent axes
  (runtime, attention, lifecycle), reduced by a pure function to a legacy
  status string `working|idle|permission|delegating|""`.
- **Track per-writer permission prompts for Claude** so that a subagent's
  blocked prompt keeps the chip red until *that writer's* prompt is proven
  resolved, and a sibling's tool completion can never clear it.
- **Bind call identity to prompts** (`tool_use_id`) lazily from the writer's own
  transcript, or eagerly from a matcher-limited `PreToolUse`, so an answered
  `AskUserQuestion` clears in under a second instead of ~17 s.
- **Discover Claude subagent fanouts** from the append-only
  `subagents/` directory and workflow journals, emit exact-once
  `subagent_spawn/stop` history events, and count in-flight children so an
  idle orchestrator renders green (delegating).
- **Observe Codex threads** through a disposable `codex app-server --stdio`
  child, classify mechanical approval gates into human-owned vs automatic
  (Auto-review) before publishing red, and reconcile that with a hook-only
  fallback for TUIs the app-server reports as `notLoaded`.
- **Fence stale results**: PID-reuse-safe root keys, per-root generation
  counters in the coordinator, connection generations in the Codex client,
  session-rotation retirement for `/clear`.
- **Survive daemon restarts**: persist prompt ownership and per-call records,
  falsify them against transcripts at hydrate, and refuse to let restored
  red/green outlive its freshness deadline.
- **Explain every decision**: a finite rule id per transition
  (`internal/statustune/knobs.go`), content-free diagnostics counters, and
  `wait_episode` telemetry for Codex ownership classification.

---

## 2. The status model

### 2.1 Values

**Node axes** (`internal/agentgraph/types.go:16-70`), applied to root and
children alike:

| Axis | Values | Notes |
|---|---|---|
| `RuntimeState` | `unknown`, `not_loaded`, `idle`, `active`, `system_error` | empty canonicalises to `unknown` (`normalize.go:123-128`) |
| `AttentionState` | `none`, `approval`, `user_input` | only a confirmed, unresolved, human-owned request may set a non-`none` value |
| `LifecycleState` | `unknown`, `pending`, `running`, `completed`, `interrupted`, `errored`, `shutdown`, `not_found` | `Terminal()` = completed/interrupted/errored/shutdown/not_found (`types.go:85-92`) |

**Root legacy status** (`agentgraph/types.go:194-199`, `state/state.go:112-117`)
and the colour each renders (`docs/status-color-state-model.md:19-26`):

| Legacy status | Colour | Meaning |
|---|---|---|
| `permission` | RED | a human-owned request is outstanding somewhere in the tree |
| `working` | GREEN | root runtime active, nothing waiting |
| `delegating` | GREEN | root not active, but a live child is active/pending/running |
| `idle` | ORANGE | root idle, no waiting, no working child |
| `""` (unknown) | GRAY | graph expired, invalid, root `not_loaded`/`unknown`, or root `system_error` |
| overlay `suspended` | grey-out | process job-control stopped (owned by discovery, not this subsystem) |
| overlay gone | hidden | process dead |

**Child agent status** is the same three axes. Provider-local sources map to
them:

| Source | Value | Runtime | Lifecycle |
|---|---|---|---|
| Claude fanout (`claude/observer.go:1055-1093`) | `pending` (meta exists, no jsonl yet) | `not_loaded` | `pending` |
| | `running` | `active` | `running` |
| | `completed` (jsonl `end_turn` / parent `tool_result` / journal `result`) | `idle` | `completed` |
| | `interrupted` (stale > 30 min cap, `SuspectStaleQuiescent`) | `idle` | `interrupted` |
| Claude hook overlay (`claude/observer.go:1199-1213`) | child `UserPromptSubmit`/`PostToolUse` → `active`; `Stop`/`PermissionRequest` → `idle` | overlay only if child is already known or holds a prompt | unchanged |
| Codex `thread.status.type` (`codex/protocol.go:899-912`) | `notLoaded`/`idle`/`active`/`systemError` | mapped 1:1 | — |
| Codex `agentsStates[*].status` (`protocol.go:914-933`) | `pendingInit`/`running`/`completed`/`interrupted`/`errored`/`shutdown`/`notFound` | — | mapped 1:1 |
| Codex child hook (`codex_hook_transitions.go:427-436`) | `SubagentStart` → `active`/`running`; `SubagentStop` → `idle`/`completed` | only when the child already exists in a fresh app-server graph | |

### 2.2 The reducer (the fold)

`agentgraph.Reduce` (`internal/agentgraph/reduce.go:9-67`) is pure. Given a
normalized observation, prior summary and `now`:

1. Not `Fresh(now)` (half-open `ObservedAt <= now < FreshUntil`,
   `types.go:163-168`) → `Runtime=unknown`, `LegacyStatus=""`.
2. Count live nodes. A non-root node is live iff `PositivelyLive`
   (`reduce.go:74-85`): not terminal, and (attention set, or runtime
   active/idle, or lifecycle pending/running). Bare topology with
   unknown/not_loaded contributes nothing.
3. `WaitingNodes = approval + user_input` over live nodes (root counts as live).
4. Fold (`reduce.go:51-64`), first match wins:
   - `WaitingNodes > 0` → `permission`
   - root `active` → `working`
   - root `system_error` → `""`
   - any live child with runtime `active` or lifecycle `pending|running` → `delegating`
   - root `idle` → `idle`
   - else → `""`
5. `Since` is carried from the prior summary only when every derived field is
   identical (`reduce.go:87-105`).

Everything else in this subsystem exists to produce the observation the fold
consumes. The state machine is therefore three machines feeding one fold:
the Claude root/prompt machine, the Codex request-ownership machine, and the
coordinator's source-precedence gate.

### 2.3 Transition table — Claude root runtime

State lives in `rootState.runtime` (`claude/observer.go:149`). Initial value
`unknown` (`ensureRootLocked`, `observer.go:1110`). Hook entry is
`applyHookLocked` (`observer.go:465-585`); transcript entry is
`reconcileRootRuntime` (`observer.go:1592-1611`) called from `Observe`
(`observer.go:339`).

| From | Event | To | Guard | Rule id | Cite |
|---|---|---|---|---|---|
| any | hook `SessionStart` (main) | `idle` | none | `root_session_started` | `observer.go:503-507` |
| any | hook `UserPromptSubmit` (agent_id empty) | `active`; `turnStartedAt=At`; retained terminal children cleared | none | `root_prompt_submitted` | `observer.go:508-517` |
| any | hook `PostToolUse` (agent_id empty) | `active` | runs after prompt matching, unconditional | see 2.4 | `observer.go:560-564` |
| any | hook `Stop` (agent_id empty) | `idle` | none | `root_stopped` | `observer.go:568-574` |
| any | hook `PreToolUse` | unchanged | stages a call-id candidate only | `hook_no_summary_change` | `observer.go:476-479` |
| any | hook `PermissionRequest` | unchanged | opens a prompt (2.4); attention, not runtime, changes | `permission_recorded_for_writer` | `observer.go:481-499` |
| any | hook `SubagentStart`/`SubagentStop` | unchanged | only invalidates the fanout snapshot | `fanout_rescan_requested` | `observer.go:579-583` |
| any | hook from a subagent (agent_id non-empty) | unchanged | child overlay only (2.6) | `child_activity_only` | `observer.go:518,565,576` |
| `idle` | Observe tick: main transcript mtime > `runtimeAt` and `NewestSignal` is `SignalActivity` dated after `runtimeAt` | `active` | assistant msg or non-interrupt, non-local-command user msg | (unattributed → `agent_graph_authority`) | `observer.go:1604-1606`, `transcript.go:1236-1257` |
| `active` | Observe tick: newest signal is `SignalInterrupt` (`[Request interrupted by user`) after `runtimeAt` | `idle` | same mtime pre-gate | unattributed | `observer.go:1607-1609` |
| any | Observe tick: main writer's whole prompt set resolved with exit runtime R | R (`active` on resumed/call resolved, `idle` on interrupted/declined/TTL/stale/child terminal) | only when the writer's set is now empty | see 2.4 | `observer.go:404-410` |
| any | `Restore` (hydrate) | `runtimeFromLegacy(status)`: working→active; idle/delegating/permission→idle; else unknown | once per tracked root | restored source | `projection.go:243-247,485-494` |
| any | `Forget` / session rotation (`ProviderSessionID` changed under same `RootKey`) | state discarded; new `rootState` at `unknown` | fanout cursors for the old session dropped | — | `observer.go:1095-1119,1648-1656` |

Note what is *absent*: no Claude timeout ever moves runtime on its own. The
H9 silent-abort recovery (pane title glyph) lives only in the legacy
reconciler (`main.go:1456-1464`) and is **not** applied to graph-authoritative
sessions.

### 2.4 Transition table — Claude prompt set (attention)

State: `rootState.pending map[writer][]PendingPrompt` (`observer.go:157`),
oldest-first per writer, capped at 32 (`observer.go:64`); `promptAnchors`
(`observer.go:174`) is a forward-only per-writer clock. Attention on a node is
`foldPromptAttention` over its set (`observer.go:876-887`): approval beats
user_input. Red leaves only at `len == 0` for every writer.

Each prompt has a latch sub-state (`projection.go:67-109`):
`Unbound → Proposed → Bound`, or terminal `Ambiguous` / `Contested`.

| From (writer W set) | Event | To | Guard | Rule id | Cite |
|---|---|---|---|---|---|
| any | hook `PermissionRequest` from W | append `{tool, hash, attention(tool), since}`; if W ≠ main, child overlay runtime `idle` | deduped on identity (tool, hash, attention, since) — a verbatim redelivery is one prompt; PreToolUse candidate joined when unique (`consumePreToolCandidate`) → latch `Bound` at open | `permission_recorded_for_writer` | `observer.go:481-499,802-819,711-762` |
| set non-empty | hook `PostToolUse` from W, exact id match on a `Bound` prompt | close that one prompt | id equality wins anywhere in the set; lifts the fanout floor | `writer_call_match_cleared` | `observer.go:526-545,857-871,1176-1178` |
| set non-empty | hook `PostToolUse` from W, shape match (tool equal, hashes equal or either empty) | close the oldest shape-matched prompt | **fanout floor**: refused when W is main (`""`) and `fanout.InFlight > 0` | `writer_tool_match_cleared` | `observer.go:539-545,1160-1171` |
| set non-empty | hook `PostToolUse` from W, both sides carry ids and they differ, no other match | hold | real negative | `writer_call_mismatch_held` | `observer.go:546-555,1187-1197` |
| set non-empty | hook `PostToolUse` from W, tool mismatch / hash mismatch / floor refused | hold | | `writer_prompt_held` | `observer.go:546-547` |
| set empty | hook `PostToolUse` from W while another writer is red | hold (no evidence about the other writer) | | `non_owner_prompt_held` | `observer.go:556-557` |
| set non-empty | hook `Stop`/`UserPromptSubmit`/`SessionStart` | hold (never clears a prompt) | plan Q6: queued messages during a prompt are common | `root_stopped` etc. | `observer.go:508-578` (no pending mutation) |
| set non-empty | Observe: fanout snapshot shows W terminal (`completed`/`interrupted`) | close whole set; exit `idle` | prompts opened after the snapshot's `ObservedAt` survive | `child_terminal` | `observer.go:1246-1248,972-990` |
| set non-empty | Observe: W's own transcript has an assistant entry dated after W's anchor | close whole set; exit `active` | `ResolveKind` newest-wins; a bare `tool_result` never counts | `writer_resumed` | `observer.go:1250-1255`, `transcript.go:597-618,683-691` |
| set non-empty | Observe: W's transcript has `[Request interrupted by user` after anchor | close whole set; exit `idle` | | `writer_interrupted` | `observer.go:1256-1258` |
| set non-empty | Observe: W is main, main transcript unreadable, `now - anchor >= PermissionDecayTTL` (30 s) | close set; exit `idle` | **main only**: a missing `agent-<id>.jsonl` is normal for a fresh child | `main_unreadable_ttl` | `observer.go:1260-1262` |
| set non-empty, some `Bound` | Observe: `tool_result` with `tool_use_id == CallID` present in W's tail | close exactly those prompts; exit `idle` iff every closed call was declined, else `active` | decline = `is_error` AND (`toolDenialKind=="user-rejected"` OR `toolUseResult` starts `"User rejected tool use"`) | `writer_call_resolved` / `writer_call_declined` | `observer.go:1271-1273,1537-1579`, `transcript.go:620-676` |
| set non-empty | Observe: W's tail has ANY unmatched `tool_use` (`BlockedByPendingTool == BlockedYes`) | hold; stale cap suppressed | a blocked writer is quiet *because* it is blocked | (hold, unlogged) | `observer.go:1275-1277`, `transcript.go:760-796` |
| set non-empty | Observe: W's file quiescent (max(anchor, mtime)) ≥ `PendingWriterStaleCap` (30 min) and no unmatched tool | close set; exit `idle` | | `writer_stale_backstop` | `observer.go:1278-1280,1581-1590` |
| any | Observe merge: W's live set ≠ the set the scan reasoned about | drop the resolution (hook won) | `slices.Equal` fence | — | `observer.go:378-382` |
| `Unbound`/`Proposed` | Observe: `PendingCall` finds exactly one unmatched `tool_use` of the tool, dated ≥ `since - 3 s`, W has exactly one unlatched prompt of that tool, no other writer claims the id | `Proposed` (first read) → `Bound` (a later read ≥ 2 s after `LatchAt` naming the same id) | | `prompt_call_latched` (diag) | `observer.go:1322-1452,1469,1487,1493-1495` |
| `Unbound`/`Proposed` | two or more candidates, or W holds two unlatched prompts of the tool | `Ambiguous` (terminal, shape rule forever) | | `prompt_call_ambiguous` | `observer.go:1428-1430` |
| any latchable | another writer offers or already holds the candidate id | `Contested` (terminal) | | `prompt_call_id_collision` | `observer.go:1414-1423` |
| `Bound`/`Proposed` | hook `PreToolUse` with a call id claimed by an incompatible candidate/prompt | `Contested`; id quarantined 60 s | | `pretooluse_call_id_collision` | `observer.go:679-709` |
| any | `Restore` from `state.json` | rebuilt: per-call records → one prompt each, `Bound` if `call_id` persisted; legacy scalar → one `Residual` prompt (never latches); anchor := restart instant; duplicated ids → `Contested` | attention repaired from tool when not approval/user_input | restored | `projection.go:230-285,392-483` |
| any | exact session rotation | whole `rootState` replaced (set gone) | | — | `observer.go:1098-1108` |

The daemon's hydrate falsifier runs *before* Restore
(`main.go:470-524`): a persisted writer whose own file shows no unmatched
`tool_use` is dropped, and a persisted record whose exact `call_id` has a
`tool_result` is dropped individually.

### 2.5 Transition table — Claude children (fanout lifecycle)

State: `fanout.sessionState` keyed by session id (`fanout/observer.go:55-73`),
and `rootState.{known, retained}` in the adapter. Source of truth is the
directory scan; the parent-transcript cursor and hooks are accelerators.

| From | Event | To | Guard | Cite |
|---|---|---|---|---|
| unseen | `agent-<id>.meta.json` or `agent-<id>.jsonl` appears under `<session>/subagents/` | spawned; `subagent_spawn` event once (seen-set seeded from history log) | keyed by filename stem; `toolUseId` best-effort | `fanout/observer.go:217-227`, `subagents.go:257-340` |
| spawned, no jsonl | jsonl exists | `running` | `pending` while path missing | `fanout/snapshot.go:172-175` |
| running | jsonl last complete line `message.stop_reason == "end_turn"` | `completed` | | `subagents.go:350-390` |
| running | parent `tool_result` for its `toolUseId` landed and the spawn was not marked background | `completed` | a launch ack (`"Async agent launched successfully"`, `"Spawned successfully"`) is NOT completion and marks the spawn background | `fanout/observer.go:185-206,245-247`, `transcript.go:263-317` |
| running | jsonl mtime older than `staleCap` (30 min) | `interrupted`, `SuspectStaleQuiescent` | | `fanout/observer.go:248-251`, `snapshot.go:160-165` |
| workflow agent started (journal `started`) | journal `result` | `completed` | never by jsonl `end_turn` (structured-output agents end on a `tool_result`) | `fanout/observer.go:339-344`, `workflows.go:168-189` |
| workflow agent started | agent transcript (or journal) mtime > `staleCap` | `interrupted` (`closed`) | | `fanout/observer.go:353-366` |
| workflow run inactive | any agent running, or journal/dir mtime within `quietGrace` (90 s) | active; `workflow_start` emitted once per episode | | `fanout/observer.go:372-392` |
| workflow run active | no agents and journal quiet > 90 s | inactive; `workflow_stop` | | `fanout/observer.go:429-434` |
| any terminal child | main `UserPromptSubmit` | dropped from `retained` (new turn cohort) | children that became terminal during this turn are retained for display; on first observation after restart only the latest-completed cohort is kept | `observer.go:511,923-967` |
| in-flight count | depth ≥ 2 | not counted (grandchildren decorate only) | | `fanout/observer.go:276-278` |

### 2.6 Transition table — Claude child overlay (hook-driven)

`childOverlay` (`observer.go:204-208`) is applied only to a child already
known to fanout or holding a prompt (`setChildRuntime`, `observer.go:1199-1213`),
and only when the overlay is at least as new as the child's `UpdatedAt`
(`observer.go:1026-1032`).

| Event (agent_id = C) | Overlay runtime | Cite |
|---|---|---|
| `PermissionRequest` from C | `idle` (blocked) + attention from its prompt set | `observer.go:491-497` |
| `UserPromptSubmit` / `PostToolUse` from C | `active` | `observer.go:518,565` |
| `Stop` from C | `idle` | `observer.go:576` |
| Observe resolves C's whole set | resolution's exit runtime | `observer.go:407-409` |

### 2.7 Transition table — Codex app-server (per node request ownership)

State: `graphState.nodes[id].{baseRuntime, wait}` (`codex/protocol.go:105-127`).
Raw flags `waitingOnApproval`/`waitingOnUserInput` are mechanical gates. Each
server request gets an owner: `Pending → Human | Automatic | Ignored`
(`protocol.go:80-87`). Derived attention (`deriveNode`, `protocol.go:349-376`):
any `Human` request → attention (approval wins); else if a raw flag is set and
not covered by auto evidence → runtime forced to `unknown` (gray, never red).

| From | Event | To | Guard | Cite |
|---|---|---|---|---|
| — | `thread/started` / `thread/updated` for root, a known node, or a child of a known node | upsert node: parent, nickname, role, `baseRuntime` from `status.type`, guardian flag from `source.subAgent.other=="guardian"` | unrelated roots on the shared stream are ignored | `observer.go:876-894`, `protocol.go:178-206,804-814` |
| — | `thread/status/changed` for unknown thread | parked in `pendingStatuses` (cap 256), applied when the thread appears | | `observer.go:1054-1062,884-887` |
| any | `thread/status/changed` with `activeFlags` | raw flags set; if flags dropped → `clearTransientWait` (requests, reviews, classification all gone) | unknown enum → diagnostic, never crash | `protocol.go:238-265` |
| root | `turn/started` (root) | `rootTurnID` advances; terminal children not in this cohort and not needed as ancestors are pruned | | `protocol.go:295-319` |
| any node | `turn/started` | `baseRuntime = active` | | `observer.go:911-918` |
| any node | `turn/completed` | all wait state and attention cleared | classification source `turn_completed` | `observer.go:919-923`, `protocol.go:730-738` |
| — | `item/started|completed` with `collabAgentToolCall` | receivers ensured as children of `senderThreadId`; `agentsStates` → lifecycle; terminal lifecycle clears wait and stamps `CompletedAt` | cohort = turn id when sender is root | `protocol.go:267-293` |
| child | `thread/archived` / `thread/deleted` | node and its subtree deleted (root: wait cleared only) | | `observer.go:937-945`, `protocol.go:321-341` |
| request `Pending` | `item/commandExecution|fileChange|permissions/requestApproval`, `applyPatchApproval`, `execCommandApproval` | request added `Pending` unless auto evidence (→ `Automatic`) or effective reviewer `user` (→ `Human`, red immediately) | request id = JSON-RPC envelope id, string or int (`parseRequestID`) | `observer.go:967-990`, `protocol.go:519-560,786-802` |
| — | `item/tool/requestUserInput` | `Human` if `isBlocking && autoResolutionMs == nil` (red immediately); `Ignored` if non-blocking; `Pending` with evidence `auto_resolving_input` otherwise | | `observer.go:991-1009` |
| — | `mcpServer/elicitation/request` | `Human`, red immediately, force publish | | `observer.go:1010-1021` |
| `Pending` | `thread/settings/updated.approvalsReviewer == "user"` | `Human` (red) for every pending request under that subtree | reviewer inherited from ancestors (`effectiveReviewer`) | `protocol.go:449-475,378-388` |
| `Pending` or timeout-`Human` | `approvalsReviewer == "auto_review"|"guardian_subagent"` | `Automatic`; red cleared if it had been published | | `protocol.go:459-460,586-599` |
| `Pending` or timeout-`Human` | `item/autoApprovalReview/started|completed` (matching `targetItemId`, or any approval request when no target) | `Automatic`, `autoReviewSeen` | | `protocol.go:477-517` |
| `Pending` | guardian-sourced child thread appears/updates | `Automatic` (`guardian_source`) | | `observer.go:888-892` |
| `Pending` | 30 s (`WaitClassification`) elapsed since request started with no ownership evidence | `Human` with evidence `timeout_fallback`; red published; `red_published` diagnostic | timer per node, token-fenced; **Phase-1 exception**, slated for removal | `observer.go:1174-1254`, `protocol.go:658-686` |
| raw gate, no request | 30 s timer expires with nothing promoted | `classifiedUnknown` → runtime `unknown` (gray) | | `protocol.go:683,688-710` |
| any owner | `serverRequest/resolved` with matching id | request removed; if raw flags still set → `classifiedUnknown` (gray until next status) | | `observer.go:1022-1028`, `protocol.go:562-577` |
| any | wait-related notification for a thread not yet in the graph | retained in `pendingWaits` (cap 256) and replayed when the thread's `thread/started|updated` arrives | | `observer.go:1069-1085,1111-1132` |
| any | reconnect | every node's wait ownership reset; queued notifications discarded; full resnapshot before notifications are applied | generation-fenced | `observer.go:346-373,778-784` |
| any | disconnect | `connected=false`; all classification timers stopped; every root signalled so the coordinator re-observes (and the freshness deadline expires the graph) | | `observer.go:721-743` |
| published | `FreshUntil` reached (15 s default) | expiry timer signals the root; reducer reads it as unknown until the next snapshot lands | | `observer.go:1392-1410` |

Publication rule (`observer.go:567,1039-1043,1224`): a snapshot or event is
published only when no node has a pending classification, OR some node has
human attention, OR the previously published graph had human attention that
is now gone (must clear stale red). Otherwise the last published observation
stands.

### 2.8 Transition table — Codex hook fallback (daemon side)

Hooks are the only exact binding source for Codex (`codex/binding.go:53-77`).
The daemon's `codexHookRootState` (`codex_hook_transitions.go:43-66`) keeps a
per-root reducer for question and approval gates, independent of the app-server.

| From | Event | To | Guard | Cite |
|---|---|---|---|---|
| — | `SessionStart` with source `startup|resume|clear` | deferred 250 ms; coalesced with the next hook for the same thread; a hook for the retired thread cannot cancel it | | `codex_hook_transitions.go:117-176` |
| — | `SessionStart` source `compact` | root `active` | | `:1332-1336` |
| — | `SessionStart` (other) / `Stop` | root `idle`, freshness 7 d | | `:1331-1338,1358` |
| — | `UserPromptSubmit` / `PreToolUse` / `PostToolUse` | root `active`, freshness 10 min | `PreToolUse` of `request_user_input` → `idle` + `user_input`, 24 h | `:1339-1344` |
| — | `PermissionRequest` of `AskUserQuestion`/`request_user_input` | root `idle` + `user_input` (red immediately) and a `codexPendingInput` record keyed by `tool_use_id` or composite | both onset edges fold into one record | `:1345-1354,873-881,1194-1206` |
| pending input | `PostToolUse` of the same tool with id match (or composite when the record has no id) | record removed | asymmetric matcher (`codexPendingInputMatches`) | `:883-890,1168-1182` |
| pending input | root `Stop` (turn id equal or either absent) | all records removed, **writer-blind** | every Stop reaching the reducer is the root turn boundary | `:891-913` |
| — | generic `PermissionRequest` | root stays `active`; a `codexPendingApproval` with a 30 s grace timer | hook cannot say who owns the decision | `:936-951,998-1072` |
| approval pending | `PreToolUse`/`PostToolUse` of the same call; root `Stop`; `SessionStart`; `UserPromptSubmit` | gate resolved, timer stopped | Stop/SessionStart/UserPromptSubmit are the only cancelling edges for a denied gate | `:953-993` |
| approval pending | 30 s timer fires, chip already red for another reason | deferred: fresh record, another grace | | `:1022-1047` |
| approval pending | timer fires, app-server graph fresh, observed after the gate, `active/none` | resolved (`app_server_non_attention`) | | `:1048-1054` |
| approval pending | timer fires otherwise | red published (`AttentionApproval`, 24 h freshness); re-asserted on every later observation until a release edge | | `:1055-1071,1274-1289` |
| any | hook for a different thread id | session rotated: approvals cleared, old id retired, child state cleared | only `SessionStart`/`UserPromptSubmit` (or a coalesced start) may rotate | `:743-775` |
| — | `SubagentStart`/`SubagentStop` | queued (cap 256) until a fresh app-server graph proves the id is a child; then an overlay of runtime/lifecycle that provider evidence at least as new retires; `SubagentStart` overlays expire after 10 min | never creates topology | `:298-355,360-413,442-530,610-741` |

### 2.9 Coordinator landing gate (who wins)

`shouldApplyObservation` (`agent_observation.go:637-673`) decides whether a new
observation replaces the stored graph, unless the caller asserts hook
ownership (`hookOwnsTransition`, `:501`):

- Different root id → accept.
- Codex: strictly by `ObservedAt` (newer wins; equal falls through), except a
  fresh current graph is not replaced by an expired observation from another source.
- Then by `sourceRank` (`:718-731`): app-server = transcript = 4 > hook = 3 >
  rollout = 2 > restored = 1. A fresh higher-rank current graph blocks a
  lower-rank candidate; a fresh higher-rank candidate wins; else newest wins.

Before the gate, every Codex observation passes four overlays in a load-bearing
order (`:483-490`): hook root status onto a `notLoaded` snapshot
(`codex_hook_transitions.go:804-835`), pending question red (`:1237-1245`),
published approval red (`:1274-1289`), then child hook overlays (`:610-680`).

### 2.10 Legacy reconciler (graph-less sessions only)

`reconcileOnce` (`main.go:969-988`) runs `selfHealStaleAttention` and
`selfHealStuckStatus` only for sessions whose `AgentGraph == nil`, which in
practice is a Claude session before its first exact hook binding. Its rules
(`case5-delegating`, `case6-idle-title`, `case9-approve-resume`,
`case10/11-decline-*`, `case15-ttl-backstop`, `case19-stale-writer-backstop`,
`resume-activity`, `case6-interrupt`) mirror 2.3/2.4 but on the flat
`AgentInfo` block. `rpc.handleHook`'s legacy branch (`rpc.go:698-1000`) is
dead code whenever the coordinator is installed (`rpc.go:694-697`,
`main.go:264`), which is always in production.

---

## 3. Core types

### Domain (worth keeping)

| Type | Cite | Role |
|---|---|---|
| `agentgraph.Node`, `Observation`, `Summary` | `agentgraph/types.go:129-192` | the neutral graph and its reduction |
| `agentgraph.RuntimeState`/`AttentionState`/`LifecycleState`/`SourceKind` | `types.go:16-106` | enums |
| `provider.RootRef`, `RootKey`, `Observer` | `provider/provider.go:16-51` | orchestration seam; `RootKey{PID, StartedAt}` is PID-reuse safe |
| `claude.PendingPrompt`, `CallLatchState`, `promptIdentity` | `claude/projection.go:30-124` | one call's wait; identity is the four hook fields |
| `claude.HookSignal`, `HookResult` | `claude/observer.go:69-103` | hook envelope in, decision + rule out |
| `transcript.ResolutionKind`, `Signal`, `BlockedEvidence`, `PendingToolCall` | `transcript.go:466-486,80-91,708-725,808-811` | verdicts a tail read can give |
| `transcript.Subagent`, `WorkflowRun`, `Task`, `TaskResult` | `subagents.go:41-52`, `workflows.go:57-62`, `transcript.go:892-898`, `subagents.go:397-400` | on-disk fanout facts |
| `fanout.Child`, `Snapshot`, `Lifecycle`, `Workflow` | `fanout/snapshot.go:18-90` | structured fanout view |
| `codex.requestEvidence`, `requestOwner`, `reviewerRoute`, `waitOwnershipState`, `graphState` | `codex/protocol.go:72-134` | request ownership machine |
| `codex.Binding`, `BindingRegistry` | `codex/binding.go:21-43` | exact thread binding per process lifetime |
| `statustune.Tuning` + rule ids | `statustune/statustune.go:40-137`, `knobs.go:8-181` | knobs and decision vocabulary |

### Plumbing (rewrite-specific, may change shape)

| Type | Cite | Role |
|---|---|---|
| `claude.rootState`, `childOverlay`, `preToolCandidate`, `promptResolution`, `promptLatch` | `observer.go:147-248` | adapter internals |
| `claude.Compatibility` | `projection.go:192-203` | legacy wire block projection (`Pending`, `PendingSets`, `PendingWriters`, `PendingTool`) |
| `claude.ShadowCase` / `CompareShadow` | `shadow.go` | migration-era oracle, now vestigial |
| `fanout.sessionState`, `workflowCursor`, `Observer` | `fanout/observer.go:55-121` | cursors and seen-sets |
| `codex.rootRecord`, `Observer`, `Config`, `WaitClassificationDiagnostic` | `codex/observer.go:67-143` | supervisor state |
| `codex.rpcClient`, `rpcEnvelope`, `rpcNotification`, `Connection`, `Connector` | `codex/client.go:34-88` | JSON-RPC codec |
| `codex.rpcThread`, `rpcStatus`, `rpcTurn`, `rpcItem`, `notificationParams` | `protocol.go:32-66`, `observer.go:833-850` | wire structs |
| `agentCoordinator`, `trackedProviderRoot`, `codexHookRootState`, `codexPendingInput/Approval`, `codexChildHookEdge/Overlay/Cursor`, `pendingCodexStart` | `agent_observation.go:53-97`, `codex_hook_transitions.go:26-100` | daemon-side orchestration and Codex hook reducer |
| `state.AgentGraph`, `AgentNode`, `AgentGraphSummary` | `state/agent_graph.go:34-98` | wire projection, `FreshUntil` ceilinged to 5 s buckets |
| `state.AgentInfo`, `PendingPrompt`, `PendingPromptRecord` | `state/state.go:188-400` | legacy enrichment block and persisted prompt records |
| `rpc.Request` | `rpc/rpc.go:48-149` | the hook wire envelope |
| `transcript.entry`, `block`, `declineSummary` | `transcript.go:144-245` | JSONL parse shapes |

---

## 4. Concurrency

### 4.1 Goroutines

| Goroutine | Started by | Stops | Communicates |
|---|---|---|---|
| `agentCoordinator.run` | `Start` (`agent_observation.go:134-146`) | ctx cancel; `Close` waits `wg` (`:148-185`) | selects on `requests`, `claude.Updates()`, `codex.Updates()`, 5 s ticker (`:202-228`); calls `Observe` (slow I/O) then `store.Apply` under `c.mu` |
| `codex.Observer.run` (supervisor) | `NewObserver` (`codex/observer.go:161`) | ctx cancel; `Close` cancels + waits (`:308-325`) | connect/backoff loop (`:327-419`); per-connection ticker (1 s active / 10 s idle, `:790-801`), stable timer, `refresh`/`cadence` chans; drives `resnapshotAll` (blocking RPC with 5 s timeout) |
| `rpcClient.readLoop` | `newRPCClientWithRequests` (`client.go:99,182-225`) | EOF/decode error → `fail` closes waiters and `done`; `Close` kills the child | delivers notifications synchronously to `handleNotification` (takes `o.mu`, signals queue, never blocks) and responses to per-request `chan rpcResponse` (cap 1) |
| stderr drain | `CommandConnector.Connect` (`proxy.go:93`) | child exit | `io.Copy(io.Discard)` |
| naming worker | `completeCodexNaming` (`agent_observation.go:1141-1152`) | ctx cancel / 45 s timeout ×2 | spawns `codex app-server --listen stdio://`, writes `sess.DisplayName` under store lock |
| `runReconciler` (legacy) | `main.go:259` | ctx | 5 s tick; touches only graph-less sessions; label + usage events |
| RPC connection handlers | `rpc.Server.handle` | per connection | `handleHook` → `dispatchAgentHook` → `coordinator.HandleHook` synchronously on the connection goroutine (`rpc.go:693-697,1005-1017`) |
| discovery scanner / pidfd watch | `main.go:236-250` | ctx | `agentRuntime.Request(key)` on appear; `forgetRoot` → `RequestCleanup` on death |

### 4.2 Timers (`time.AfterFunc`)

| Timer | Per | Delay | Cite |
|---|---|---|---|
| Codex observation expiry | root record | `FreshUntil - now` (15 s) | `codex/observer.go:1392-1410` |
| Codex classification | node with pending ownership | min remaining of 30 s grace across pending requests | `:1174-1254` |
| Codex hook approval grace | pending approval record | 30 s (re-armed on deferral) | `codex_hook_transitions.go:998-1072` |
| Codex SessionStart settle | root | 250 ms | `:129-156` |
| reconnect backoff | supervisor | 100 ms → 5 s exponential + jitter | `:1444-1454` |

All Codex timers are fenced by identity checks (record pointer, generation,
token) inside the callback, and `codexTimerWG` balances hook timers so `Close`
can wait for them.

### 4.3 Mutexes

- `claude.Observer.mu` (`observer.go:138`): protects `roots`; **never held
  across I/O** — `Observe` unlocks, reads disk, relocks and fences by
  pointer identity and session id (`:319-329,344-364`).
- `fanout.Observer.mu` (`fanout/observer.go:106`): held across `Reconcile`'s
  directory scan and tail reads (I/O under lock, by design a leaf).
- `codex.Observer.mu` (`codex/observer.go:129`): protects roots, generation,
  queued notifications; `resnapshotAll` copies targets out then RPCs unlocked.
- `rpcClient.mu` / `writeMu` (`client.go:79-80`).
- `BindingRegistry.mu` RWMutex (`binding.go:41`).
- `agentCoordinator.mu` (generation, tracked, diagnostics), `namingMu`,
  `codexHookMu` (`agent_observation.go:73,84,90`).
- `state.Store` lock via `Apply`/`Snapshot`; the coordinator holds `c.mu`
  while calling `store.Apply` (`agent_observation.go:491-557`).

### 4.4 Channels

- `provider.InvalidationQueue` (cap 64, drop-on-full, coalesced by key while
  queued; `provider.go:57-99`) — three instances: Claude updates, Codex
  updates, coordinator requests.
- Codex `refresh` and `cadence` (cap 1, `observer.go:141-142`).
- `rpcClient.waiters[id] chan rpcResponse` (cap 1) and `done chan error` (cap 1).
- Namer `message`/`completed` (cap 1, `namer.go:41-42`).

### 4.5 Blocking I/O per Observe tick (Claude)

No inotify anywhere; all file evidence is polled. One Claude `Observe`
(`observer.go:333-339`) performs, for one root:

1. `fanout.Observe` → `Reconcile`: `os.Stat` transcript, `TasksSince` forward
   read of new bytes, `ReadDir` on `subagents/`, `ReadFile` each
   `*.meta.json`, 128 KiB tail read of each `agent-*.jsonl`, `ReadDir`
   `subagents/workflows/`, `ReadDir` `workflows/scripts/`, per-run journal
   forward read and per-agent `os.Stat`; then `snapshotLocked` repeats the
   `SubagentsForTranscript` scan (`snapshot.go:133`) — the directory is scanned
   twice per tick.
2. `latchPendingCalls`: one 128 KiB tail read per (writer, tool) with a
   latchable prompt.
3. `resolvePending`: per blocked writer, `ResolveKind` tail read, then
   `ResolveKindForCalls` tail read, then `BlockedByPendingTool` tail read,
   then `os.Stat` — up to three full decodes of the same 128 KiB.
4. `reconcileRootRuntime`: `os.Stat` + `NewestSignal` tail read.

Triggered by every hook edge from any writer (`ApplyHook` signals,
`observer.go:458`) plus the 5 s tick, so a fanned-out session reads its
transcripts many times per second.

### 4.6 Data flow

```
 Claude Code process                        Codex TUI process
 ├─ writes ~/.claude/projects/<slug>/       ├─ spawns hook subprocess ──► switchboard-ctl codex-hook <Event>
 │   <sid>.jsonl, <sid>/subagents/*         │
 └─ spawns hook subprocess                  └─ (thread also readable by a disposable
      │ stdin JSON                                `codex app-server --stdio` child)
      ▼
 switchboard-ctl hook <Event>  (parseHookPayload: hash tool_input, drop raw)
      │ unix socket, rpc.Request{Event, PID=getppid, SessionID, Transcript,
      │   AgentID, ToolName, ToolInputHash, ToolUseID, TurnID, HookSource}
      ▼
 rpc.Server.handleHook ─► dispatchAgentHook: findTrackedAncestor(ppid walk, stop at
      │                   first agent process; untracked ⇒ drop)
      ▼
 agentCoordinator.HandleHook (RPC goroutine)
   ├─ claude: restoreClaude (once) → begin(gen) → claude.ApplyHook (mu, no I/O)
   │          → HookResult{Observation, Projection, Rule} → applyObservationWithRule
   └─ codex:  SubagentStart/Stop → enqueueCodexChildHook (queue until graph proves child)
              SessionStart(startup|resume|clear) → 250 ms settle timer
              else → handleCodexHookNow: binding registry, pending input/approval
                     reducers, codexHookObservation → overlays → applyObservationWithHookOwnership
                     → c.Request(key)

 coordinator.run (single goroutine)  ◄── InvalidationQueue ◄── claude.ApplyHook / codex snapshot|event|expiry / discovery
   └─ observeAt: observer.Observe(ctx, ref, now)
        claude: fanout scan + transcript tails (unlocked) → merge under mu → rebuildLocked
                → Normalize → Reduce → projectCompatibility
        codex:  bindings.resolve → return last published snapshot (no I/O)
      → DrainResolutionRule / DrainPromptDiagnostics / Projection
      → applyObservationWithRule: codex overlays → shouldApplyObservation gate
        → state.ProjectAgentGraph (Normalize+Reduce again) → store.Apply(SetAgentGraph,
          applyClaudeCompatibility) → history transition event + statustune.Decision log

 codex.Observer.run (supervisor goroutine)
   connect(codex --version, codex app-server --stdio) → initialize → resnapshotAll
   (thread/read, thread/turns/list, thread/list ancestorThreadId paged) → installSnapshot
   readLoop goroutine → handleNotification → applyNotificationLocked → graphState mutation
   → classification timers → queue.Signal(key)
```

---

## 5. External boundaries

### 5.1 Files and processes

| Boundary | Pattern | Reader |
|---|---|---|
| Claude main transcript | `transcript_path` from the hook, i.e. `~/.claude/projects/<cwd-slug>/<session-uuid>.jsonl` | tail reads (`transcript.go:364-407`), forward cursor (`:1053-1083`) |
| Subagent files | `<dir>/<session-uuid>/subagents/agent-<id>.meta.json` and `agent-<id>.jsonl` (derived only from the transcript path, `subagents.go:95-100`) | `SubagentsForTranscript`, `SubagentPath`, `SubagentMetaPath` |
| Workflow runs | `<dir>/<session-uuid>/subagents/workflows/wf_<hex>/{journal.jsonl,agent-<id>.jsonl,agent-<id>.meta.json}` and `<dir>/<session-uuid>/workflows/scripts/<name>-wf_<hex>.js` | `workflows.go` |
| History log | `$XDG_STATE_HOME/switchboard/history/<day>.jsonl` (seed index for exact-once spawn events) | `fanout/seedtelemetry.go:36-57` |
| State file | `$XDG_CACHE_HOME/switchboard/state.json` | hydrate (`main.go:397-445`) |
| Hook socket | `$XDG_RUNTIME_DIR/switchboard.sock` | `rpc` |
| `/proc` | ppid walk, depth ≤ 20 (`rpc.go:1407-1422`) | attribution |
| Codex CLI | `codex --version` (must be ≥ 0.149.0, `proxy.go:15,103-122`), `codex app-server --stdio` (observer), `codex app-server --listen stdio://` (ephemeral namer) | `proxy.go` |
| Claude hook registration | `~/.claude/settings.json`: SessionStart, UserPromptSubmit, PreToolUse (matcher `AskUserQuestion\|ExitPlanMode`), PostToolUse, PermissionRequest, Stop, SubagentStart, SubagentStop → `switchboard-ctl hook <Event>` | README:352-359 |
| Codex hook registration | `~/.codex/hooks.json`: same eight events → `switchboard-ctl codex-hook <Event>` | README:397-404 |

### 5.2 Claude transcript JSONL shapes that matter

Parsed by `entry`/`block` (`transcript.go:144-245`). Only these fields are read.

Assistant dispatch (one line per content block; the `tool_use` id is the call
identity; `timestamp` is generation time, flushed ~5 s later):

```json
{"type":"assistant","timestamp":"2026-08-05T19:38:14.068Z","message":{"role":"assistant","model":"claude-opus-4-8","stop_reason":"tool_use","usage":{"input_tokens":2,"output_tokens":2504,"cache_read_input_tokens":154510,"cache_creation_input_tokens":889},"content":[{"type":"tool_use","id":"toolu_01ABC","name":"Bash","input":{"command":"git status"}}]}}
```

User tool result (clean):

```json
{"type":"user","timestamp":"2026-08-05T19:42:55.100Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01ABC","content":"..."}]}}
```

User tool result, human declined (the only decline signal, `transcript.go:650-676`):

```json
{"type":"user","timestamp":"...","toolUseResult":"User rejected tool use","toolDenialKind":"user-rejected","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_01ABC","is_error":true,"content":"..."}]}}
```

Interrupt marker (fires no Stop hook; `transcript.go:109`):

```json
{"type":"user","timestamp":"...","message":{"role":"user","content":[{"type":"text","text":"[Request interrupted by user]"}]}}
```

Local `!`/`/` command (not agent activity; `transcript.go:132`):

```json
{"type":"user","timestamp":"...","message":{"role":"user","content":"<bash-input>git status</bash-input>"}}
```

Subagent spawn and its launch ack (ack ≠ completion, `transcript.go:263-266`):

```json
{"type":"assistant","timestamp":"...","message":{"role":"assistant","content":[{"type":"tool_use","id":"toolu_02","name":"Agent","input":{"subagent_type":"Explore","description":"survey x","run_in_background":false}}]}}
{"type":"user","timestamp":"...","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_02","content":"Async agent launched successfully. The agent is working in the background..."}]}}
```

Subagent transcript terminal line: last complete line with
`message.stop_reason == "end_turn"` (`subagents.go:233`).

`agent-<id>.meta.json` (heterogeneous; only `agentType` universal, `subagents.go:61-68`):

```json
{"agentType":"general-purpose","name":"escalate-cleanup","description":"...","toolUseId":"toolu_02","spawnDepth":1,"taskKind":"in_process_teammate"}
```

Workflow journal lines (`workflows.go:144-158`):

```json
{"type":"started","key":"a1","agentId":"a9f3e..."}
{"type":"result","key":"a1","agentId":"a9f3e...","result":{...}}
```

### 5.3 Hook payload shapes

Claude stdin (verified against 2.1.222, `docs/claude-code-hook-schema.md:18-107`):
every event carries `session_id`, `transcript_path` (**always the parent's**),
`cwd`, `permission_mode`, `agent_id` (only inside a subagent, bare form),
`agent_type`. Tool events add `tool_name`, `tool_input`; `PreToolUse` and
`PostToolUse` add `tool_use_id`; **`PermissionRequest` does not**. `tool_input`
differs between `PermissionRequest` and `PostToolUse` on approval-rewrite paths.

Codex stdin adds `source` (startup/resume/clear/compact), `turn_id`,
`tool_use_id`, `prompt` (UserPromptSubmit), `last_assistant_message` (Stop).

ctl reduces `tool_input` to a 16-hex-char sha256 of the canonical JSON, and
empty/null/unparseable inputs hash to `""` meaning no signal
(`switchboard-ctl/main.go:778-804`). The `rpc.Request` fields are at
`rpc.go:67-135`.

### 5.4 Codex app-server protocol

Requests (allowlisted, `client.go:17-22`): `initialize` (with
`experimentalApi:true`), `thread/read` (`includeTurns:true`),
`thread/turns/list` (limit 100 desc, optional), `thread/list`
(`ancestorThreadId`, `sourceKinds:[subAgent,subAgentReview,subAgentCompact,subAgentThreadSpawn,subAgentOther]`,
`useStateDbOnly:true`, paged by `nextCursor`). Notification `initialized`.

Notifications consumed (`observer.go:875-1029`): `thread/started`,
`thread/updated`, `thread/status/changed`, `thread/name/updated`,
`turn/started`, `turn/completed`, `item/started`, `item/completed`
(`collabAgentToolCall` with `tool` ∈ spawnAgent/sendInput/resumeAgent/wait/closeAgent,
`senderThreadId`, `receiverThreadIds`, `agentsStates`), `thread/archived`,
`thread/deleted`, `thread/settings/updated` (`approvalsReviewer`),
`item/autoApprovalReview/started|completed` (`reviewId`, `targetItemId`),
server requests `item/commandExecution/requestApproval`,
`item/fileChange/requestApproval`, `item/permissions/requestApproval`,
`applyPatchApproval`, `execCommandApproval`, `item/tool/requestUserInput`
(`isBlocking`, `autoResolutionMs`), `mcpServer/elicitation/request`, and
`serverRequest/resolved` (`requestId`). Sample lines live in
`internal/provider/codex/testdata/captures/*.jsonl`; e.g.

```json
{"method":"thread/status/changed","params":{"threadId":"...0002","status":{"type":"active","activeFlags":["waitingOnApproval"]}}}
{"id":"approval-1","method":"item/commandExecution/requestApproval","params":{"threadId":"...0002","turnId":"...0011","itemId":"...0030"}}
{"id":"user-input-1","method":"item/tool/requestUserInput","params":{"threadId":"...0003","turnId":"...0012","itemId":"...0031","isBlocking":true,"autoResolutionMs":null}}
```

The observer never answers server requests; it only records their envelope id.

---

## 6. Dependencies

Inbound to this subsystem (who calls it): `cmd/switchboard/main.go` builds the
observers (`:172-192`), wires `HandleHook` into RPC (`:264`), runs the legacy
reconciler; `internal/rpc` forwards hooks and asks `Diagnostics()`; `internal/state`
hydrates and projects graphs; renderers read `AgentInfo.Status`,
`AgentGraph.Summary`, `PendingWriters`, `Workflows`.

Outbound (what it uses): `internal/state` (Store, AgentGraph projection,
AgentInfo), `internal/history` (sink, seed index, AgentStateProjector),
`internal/statustune` (tuning, rule ids, decision log), `internal/proc` and
`internal/discovery` (ppid walk classification), `internal/rpc` types,
`os/exec` for Codex, stdlib only in `transcript` and `agentgraph`.

Package graph: `agentgraph` ← `provider` ← {`claude`, `codex`};
`transcript` ← `fanout` ← `claude`; `claude` also ← `history`, `statustune`;
`fanout` ← `state`, `history` (the one leaf-to-state edge that a rewrite
should cut).

---

## 7. Invariants and hazards

Mined from tests (`rg '^func Test'` across the packages) and the docs.

**Error-cost ranking is the design axiom** (`status-color-state-model.md:285-320`):
missed RED > persistent false RED > false GREEN > false ORANGE > transients.
Every guard below trades toward stale red, never toward missed red.

1. **Prompt ownership is per writer; only the owning writer's evidence closes it.**
   Writer key = bare `agent_id`, `""` = main. Tests:
   `TestTeammateHookCannotCloseAnyOfAnotherWritersParallelPrompts`,
   `TestObserveResolvesEachPromptAgainstItsWritersTranscript`,
   `TestChildAttentionOwnershipAndWriterCollision`.
2. **Red leaves only when every writer's set is empty**; one answer among
   parallel prompts holds. `TestParallelPromptsFromOneWriterHoldRedUntilEveryCallIsAnswered`,
   `TestEveryCallStaysOpenWhenOneTurnDispatchesPastTheMeasuredParallelMaximum`.
3. **The fanout floor**: main-thread shape matches never clear while children
   are in flight; only an id match may.
   `TestIDMatchedClearShouldLiftTheFanoutFloorWhileAShapeMatchStillHolds`,
   `TestMainToolMatchHoldsWithLiveTeammateAndHashMismatch`,
   `TestFanoutFloorHoldShouldNotBeReportedAsACallMismatch`.
4. **Non-tool hooks never clear a prompt** (Stop/UserPromptSubmit/SessionStart),
   because a queued message during a prompt is common (plan Q6).
5. **A bare `tool_result` is never resolution**; only an assistant entry or the
   interrupt marker dated after the writer's anchor is (`transcript.go:683-691`).
6. **Anchors only move forward** (`writerResolutionAnchor`,
   `observer.go:916-921`); a call-scoped clear must not re-expose the dispatch
   entry as fresh evidence. `TestRetiringTheNewestPromptShouldNotResolveAnOlderOneOnItsOwnDispatch`.
7. **Two clocks after restart**: record `since` = real onset (for latching),
   writer anchor = restart instant (for whole-file rules).
   `TestRestoreRebuildsAWritersWholeOpenSet`, `TestRestoredPromptShouldNeverBindACallID`.
8. **Latch strictness**: bind only on (one unlatched prompt of the tool) ∧
   (one eligible unmatched call) ∧ (call not older than `since - 3 s`) ∧ (no
   cross-writer claim) ∧ (confirmed by a read ≥ 2 s after the proposal).
   `TestLatchShouldRefuseACallThatWasAlreadyRunningWhenThePromptOpened`,
   `TestLatchShouldRefuseAConfirmingReadTakenInsideTheFlushGap`,
   `TestLatchShouldRefuseACandidateTheNextTickNoLongerAgreesOn`,
   `TestPromptShouldStayUnboundForeverWhenTheWritersTailHoldsTwoCallsOfTheSameTool`,
   `TestNoCallIDShouldBeClaimedByTwoWritersAcrossOneSession`,
   `TestLatchShouldRefuseAndCountACallIDTwoWritersClaim`.
9. **PreToolUse never opens red**; it only stages identity, joined on a unique
   (writer, tool, hash). `TestPreToolUseNeverOpensRedByItself`,
   `TestPreToolUseParallelIdenticalCallsAreNeverGuessed`,
   `TestPreToolUseSameShapeCallsStayWithinTheirWriters`,
   `TestPreToolUseDuplicateDeliveryIsIdempotentButCallIDCollisionIsQuarantined`.
10. **Decline ≠ error**: 204 of 218 `is_error` denials are auto-denials; only
    `user-rejected` returns control to the user.
    `TestDeclinedCallShouldExitToIdleWhileAFailedOneResumes`,
    `TestDeclinedCallShouldSetTheExitOnlyWhenEveryClosedCallDeclined`.
11. **Hook-vs-transcript clock skew** (`docs/timing-hazards.md`): into working,
    anchor to the transcript entry (H1); into idle/permission, anchor to
    wall-clock now because the turn's own entries flush *after* the hook (H7,
    H8). The graph adapter sidesteps this by stamping `runtimeAt = signal.At`
    and requiring transcript signals strictly after it.
12. **Oscillation**: teammate `PostToolUse` must not drive the root runtime;
    the idle-title demotion needs a fresh `StatusSince`. Root cause of the
    2026-08-05 limit cycle (`subagent-permission-oscillation.md:157-245`);
    fixed by writer attribution and the anchor floor.
13. **Session rotation** (`/clear`): same PID, new session id retires every
    prompt; a retired Codex thread id can never rotate the binding backwards.
    `TestExactSessionRotationRetiresOldPromptOwnership`,
    `TestBindingRegistryRotatesForwardAndFencesRetiredThread`.
14. **Attribution by ppid stops at the nearest agent process**; an untracked
    one drops the hook (the nested `claude -p` race, `rpc.go:1379-1406`). A
    shared Codex app-server daemon breaks attribution entirely
    (`codex-app-server-hook-attribution-incident.md`); never bind by cwd.
15. **Codex red requires ownership evidence**; a mechanical gate alone is
    gray, and elapsed time is only a Phase-1 fallback.
    `TestProductionShapeResolvesInsideGraceWithoutPublishingRed`,
    `TestUnknownMechanicalWaitExpiresGrayWithoutAttention`,
    `TestLateAutomaticEvidenceOverridesTimeoutFallbackAndReportsFalseRed`,
    `TestTimeoutFallbackIsNotReportedAsSemanticHumanEvidence`. Rollout
    invariant: `red_cleared_without_human_evidence = 0`.
16. **Codex ordering tolerance**: status before metadata, spawn before thread,
    wait before thread, reconnect superseding queued events, unrelated roots.
    `TestStatusBeforeMetadataIsGenerationScopedAndAppliedAfterParentage`,
    `TestOrderingNestedParentageUnknownEnumsAndTurnPruning`,
    `TestObserverReconnectFreshnessGenerationAndCoalescing`.
17. **Codex hook Stop sweeps are writer-blind on purpose** (D1 in
    `askuserquestion-model-plan.md:360-376`): Codex does not guarantee
    `agent_id` on child tool hooks, so a writer-scoped sweep would strand a red.
18. **Published Codex approval red is re-asserted until a release edge**
    (`overlayCodexApprovalObservation`), accepting a loud stale red for a
    denied gate over a silent missed one.
19. **Freshness is authority**: a restored or disconnected graph reduces to
    unknown at `FreshUntil`; a hook-only Codex `user_input` red does not
    survive hydrate (`state/agent_graph.go:312-321`).
    `TestExpiredObservationClearsAuthorityWithoutDroppingRoot`,
    `TestCodexHookFallbackFreshnessIsBoundedByState`.
20. **Generation fences**: `c.generation[key]` bumped by `begin` before any
    slow observe; stale results are dropped (`agent_observation.go:493-495`).
    `TestAgentObservationDoesNotHoldStoreLockAndFencesLateGeneration`,
    `TestOlderObservationCannotOverwriteNewerHookState`,
    `TestNewerCodexHookIsImmediateAndLaterAppServerCorrectsIt`.
21. **Exact-once history events across restarts** via the seed index and
    session-keyed seen-sets. `TestReconcile_seedsFromHistoryNoReemitOnRestart`,
    `TestReconcile_shouldNotReannounceWorkflowAfterDaemonRestart`.
22. **Launch acks are not completions**; `run_in_background` is never set on
    `Agent` spawns. `TestObserveKeepsAsyncLaunchAcknowledgementRunning`,
    `TestReconcile_backgroundIgnoresSpawnAckResult`.
23. **Never strip `agent-` twice**: a subagent named `gent-foo` lives at
    `agent-agent-foo-<hex>.jsonl`; normalisation happens once at the adapter
    (`observer.go:1215-1221`). `TestClaudeHookAgentIDIsNormalizedExactlyOnceByAdapter`.
24. **Privacy**: raw `tool_input`, prompts, paths and thread ids never enter
    diagnostics; only finite categories and short hashes.
25. **Known open holes** (`askuserquestion-model-plan.md:788-807`): a call
    answered during daemon downtime cannot be falsified unless its id was
    persisted; latch ambiguity resets on restart; the first Observe after a
    restore is always superseded; the H9 silent abort has no graph-path recovery.

---

## 8. Essential vs accidental

### Intrinsic (any rewrite must reproduce)

- The three-axis node model and the pure fold in `Reduce`. This is the one
  piece of the design that is finished and should be ported verbatim.
- Per-writer, per-call prompt ownership with "only the owner's evidence
  closes it" and "red leaves at empty set." The 2026-08-05 incident proves a
  scalar cannot express this.
- The asymmetry that hooks are edges and lossy: decline and interrupt fire no
  hook, so a second evidence source (the transcript) is unavoidable for Claude.
- Call identity: `PermissionRequest` lacks `tool_use_id`, so identity is
  either joined from `PreToolUse` or latched from the writer's own file, with
  the flush-lag and parallel-dispatch guards that follow.
- Codex ownership classification: an approval boundary is not human attention
  under Auto-review. Exact request-id correlation and reviewer settings are
  the real signals; the 30 s timeout is explicitly temporary.
- Exact identity everywhere: `RootKey{pid, started_at}`, session id from hooks,
  Codex thread id from hooks, never cwd or recency.
- Freshness deadlines as authority, and generation fencing between slow
  observation and fast hook landing.
- The append-only `subagents/` directory as the fanout source of truth, with
  launch acks distinguished from results.

### Accreted (should collapse)

- **Two parallel Claude engines.** The legacy `AgentInfo` reconciler
  (`main.go:1180-1553`) and `rpc.handleHook` (`rpc.go:698-1000`) duplicate
  the graph adapter's rules with a second rule vocabulary (`case*` vs graph
  ids). The legacy path only runs for a session with no graph yet, which is
  a window of one hook. Delete it; a session with no exact binding is simply
  gray.
- **Three copies of the prompt set.** `rs.pending` (adapter), `Compatibility.Pending`
  + `PendingSets` (projection), `AgentInfo.Pending` + `PendingPrompts` +
  `PendingWriters` + `PendingTool` (state). Keep one persisted list of
  `{writer, tool, hash, call_id, attention, since}`; derive everything else at
  the wire.
- **Two fanout APIs.** `fanout.Reconcile` mutates a `state.AgentInfo` and
  emits legacy events; `fanout.Observe` wraps it and rebuilds a `Snapshot`,
  scanning the directory twice. One structured scan that yields both the
  snapshot and the delta events is enough.
- **Normalize/Reduce run twice per landing** (`rebuildLocked` then
  `state.ProjectAgentGraph`), and the observation is cloned four or five
  times between adapter and store.
- **Codex has two overlapping reducers**: the app-server `graphState` and the
  daemon-side hook reducer with its own approval grace timer, question
  records, root-node cache and child-hook queue, fused by four ordered
  overlays plus `shouldApplyObservation`. The hook reducer exists because the
  standalone app-server reports TUIs as `notLoaded` and cannot see
  `request_user_input`. In a rewrite, hooks and app-server events should be
  two event streams into one Codex state machine with explicit per-fact
  provenance and expiry, not two graphs merged after the fact.
- **The Restore/Compatibility shadow machinery** (`shadow.go`, `CompareShadow`,
  `priorSummary.Since` patching in `Restore`) was scaffolding for the
  legacy-to-graph flip and is inert now.
- **Ad hoc timers** (expiry, classification, approval grace, settle) each with
  their own fencing idiom. A single timer wheel keyed by `(root, purpose)`
  with generation stamps replaces all four patterns.
- **Diagnostics plumbing**: `promptDiagnostics` buffers drained by the
  coordinator, `lastDiagnostics` rate limiting, `WaitClassificationDiagnostic`
  with twelve fields. Worth keeping as a typed event stream, not as
  per-adapter buffers.
- **Tail re-reads**: three to five 128 KiB decodes of the same file per
  writer per tick. One decoded tail per file per tick, cached by
  `(path, size, mtime)`, feeds every rule.

### What a clean event-sourced design looks like

Treat everything that arrives as an immutable event with a source, a root
key, a monotonic sequence and a wall-clock stamp:

```
enum Event {
  Hook(HookEvent),                    // from switchboard-ctl, exact fields only
  TranscriptDelta(Writer, Vec<Entry>),// new complete lines since a byte cursor
  FanoutScan(Vec<ChildFact>),         // directory facts, mtime-stamped
  CodexRpc(Notification | SnapshotResult),
  ProcessDied(RootKey),
  Tick(Instant),
}
```

A per-root reducer `fn apply(state, event) -> (state, Vec<Effect>)` owns all
transitions in 2.3–2.8; the derived `Observation` is a pure view of the state
plus `now`. Timers become `Effect::Schedule(purpose, at)` and fire as `Tick`
events, so every guard in 2.7/2.8 is testable without sleeping. Freshness,
generation fences and "hook owns transition" disappear: ordering is the
sequence number, and precedence is a rule in the reducer rather than a gate at
the store. The transcript becomes a proper tailer (byte cursor + `notify`
watch with a poll fallback) that emits deltas once, replacing the four
independent tail readers.

---

## 9. Rust mapping sketch

### Crates

- `serde`, `serde_json` (transcript lines, hook stdin, Codex JSON-RPC; use
  `RawValue` for `content`/`source` the way Go uses `json.RawMessage`).
- `tokio` (child process stdio, timers, unix socket) — the Codex client and
  the coordinator loop are naturally async; transcript reads are small and
  bounded, so `spawn_blocking` or plain `std::fs` behind a channel is fine.
- `notify` for the Claude transcript and `subagents/` directory, with a
  periodic rescan backstop (the Go code relies on the backstop alone).
- `thiserror` for typed errors; `tracing` for the content-free diagnostic
  categories; `sha2`+`hex` for the tool-input hash at the ctl edge.
- `proptest` or table fixtures for the reducer truth tables (the Go suite's
  `TestReduceTruthTable` and the Codex capture fixtures port directly).

### Modules

```
switchboard/
  agentgraph/     types.rs (Runtime, Attention, Lifecycle, Node, Observation, Summary)
                  normalize.rs, reduce.rs, prune.rs          — pure, no I/O
  provider/       mod.rs: trait Provider, RootRef, RootKey, InvalidationQueue
    claude/       hooks.rs (HookEvent, HookOutcome+Rule)
                  prompts.rs (PromptSet per writer, PendingPrompt, Latch)
                  latch.rs (candidate rules, skew/confirm grace)
                  resolve.rs (whole-file + call-scoped resolution)
                  fanout.rs (directory scan → ChildFact, cohort retention)
                  root.rs (root runtime + rebuild → Observation)
    codex/        client.rs (JSON-RPC codec, allowlist, generation)
                  supervisor.rs (connect/backoff/resnapshot)
                  graph.rs (graphState, request ownership, classification)
                  hooks.rs (question/approval/child-hook reducer, currently in cmd/)
                  binding.rs
  transcript/     tail.rs (bounded tail + forward cursor), entry.rs (serde shapes),
                  resolution.rs, subagents.rs, workflows.rs, usage.rs
  coordinator/    mod.rs (single task: events in, store writes out), precedence.rs
  statustune/     tuning.rs, rules.rs (enum Rule with Display = wire string)
```

### Core enums

```rust
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Runtime { Unknown, NotLoaded, Idle, Active, SystemError }
pub enum Attention { None, Approval, UserInput }
pub enum Lifecycle { Unknown, Pending, Running, Completed, Interrupted, Errored, Shutdown, NotFound }
pub enum LegacyStatus { Working, Idle, Permission, Delegating }   // Option<> for unknown

pub enum Latch { Unbound, Proposed { call: CallId, at: Instant }, Bound(CallId), Ambiguous, Contested }

pub struct PendingPrompt { tool: ToolName, hash: Option<InputHash>, attention: Attention,
                           since: SystemTime, latch: Latch, residual: bool }

pub enum ClaudeHook { SessionStart, UserPromptSubmit, PreToolUse { tool, hash, call: CallId },
                      PermissionRequest { tool, hash }, PostToolUse { tool, hash, call: CallId },
                      Stop, SubagentStart, SubagentStop }
pub struct HookEvent<H> { root: RootKey, writer: Writer /* Main | Agent(String) */, hook: H, at: SystemTime }

pub enum Resolution { None, Resumed, Interrupted, Declined }
pub enum Blocked { Unknown, Yes, No }

pub enum RequestOwner { Pending, Human(HumanEvidence), Automatic(AutoEvidence), Ignored }
pub enum Rule { PermissionRecordedForWriter, WriterCallMatchCleared, /* … one variant per knobs.go id */ }
```

Make `Writer` a newtype with `Writer::Main` distinct from an empty string,
and `CallId`/`ToolName`/`InputHash` newtypes so the "empty hash is no
signal" rule is a type-level `Option`, not a convention.

### The Provider trait

```rust
#[async_trait]
pub trait Provider: Send + Sync {
    fn kind(&self) -> ProviderKind;
    /// Ingest an exact hook edge; must not perform I/O. Returns the outcome and rule.
    fn apply_hook(&self, event: HookEvent<Self::Hook>) -> HookOutcome;
    /// Slow observation (files, RPC). Deep-copied result; may return Superseded.
    async fn observe(&self, root: &RootRef, now: SystemTime) -> Result<Observation, ObserveError>;
    fn updates(&self) -> broadcast::Receiver<RootKey>;   // coalesced invalidations
    fn forget(&self, key: RootKey);
    async fn close(self: Box<Self>);
}
```

Rather than one trait with both `apply_hook` and `observe`, the cleaner cut
is a per-provider reducer `fn step(&mut RootState, Event) -> Vec<Effect>`
behind a small `Provider` that only knows how to *produce* events (tail the
transcript, read the RPC stream). The coordinator then runs one task per root
that drains an ordered event channel, so every fence in
`agent_observation.go:450-495` becomes sequence ordering.

### Sync vs async

- `agentgraph`, `transcript` parsing, prompt/latch/resolution rules, Codex
  `graphState`: **sync, pure, `&mut self`**, unit-tested with table fixtures.
- Codex JSON-RPC client and supervisor, hook socket server, timers: **tokio**.
- Transcript and directory reads: sync `std::fs` inside `spawn_blocking`, or a
  dedicated blocking thread per root fed by `notify` events; never under any
  lock that the reducer holds.
- Coordinator: one task owning `HashMap<RootKey, RootState>` with an mpsc
  inbox; no `Mutex` around state at all. Store writes go out as messages.
- Persist the Claude prompt set and Codex bindings as the only durable
  provider state; everything else is re-derived on restart, exactly as the Go
  code already argues in `subagent-permission-plan.md §9`.
