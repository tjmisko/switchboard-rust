# Survey 10 — What a daemon can learn from a Claude Code transcript

Read-only census of the transcripts on this machine (`~/.claude/projects/**/*.jsonl`),
written 2026-09-11 against Claude Code 2.1.222–2.1.268 line versions. The Go daemon
today reads only: `type`, `timestamp`, `tool_use`/`tool_result` ids, `stop_reason`,
`usage`, `toolDenialKind`, the `[Request interrupted by user]` marker, and local
`!`/`/` commands (survey 03 §5.2). This document lists everything else on disk, what
fact each field establishes, and how quickly it lands.

Privacy note: no prompt text, assistant prose, or file contents were copied. Quoted
values are structural (enum values, keys, tag names, sentinel strings). Examples are
scrubbed: string values in content-bearing keys are replaced by `<str:N>` (N = length).

## 1. Corpus census

| Measure | Value |
|---|---|
| Transcript files (`*.jsonl`) | 2,374 |
| Total bytes | 1,106,867,292 (1.03 GiB) |
| Largest single file | 20.2 MB |
| mtime range | 2026-08-12 17:13 → 2026-09-11 09:44 (PDT) |
| Files by last-modified month | Aug 2026: 1,663; Sep 2026: 711 |
| Line `version` range | 2.1.222 … 2.1.268 (27 distinct) |

`fd` needs `-I -H` here: a global ignore rule hides `~/.claude`. Path shapes:

| Shape | Count | Meaning |
|---|---|---|
| `<project>/<sessionUuid>.jsonl` | 370 | root session transcript |
| `<sessionUuid>/subagents/agent-<id>.jsonl` | 51 | unnamed subagent (`Agent` tool) |
| `<sessionUuid>/subagents/agent-a<name>-<hex16>.jsonl` | ~155 | named subagent; the `name` param is embedded in `agentId` |
| `<sessionUuid>/subagents/workflows/wf_<id>/agent-<id>.jsonl` | 1,716 | workflow subagent (`sessionKind:"bg"`, `entrypoint:"sdk-cli"`) |
| `<sessionUuid>/subagents/workflows/wf_<id>/journal.jsonl` | 81 | workflow journal (`launched`/`started`/`result`/`failed`) |

Top-level `type` histogram over every line in the corpus (271k lines):

```
136897 assistant        2765 queue-operation       84 failed
 91415 user             2584 ai-title              84 cost-state
 40316 attachment       2519 permission-mode       28 frame-link
  4808 last-prompt      2308 system                24 artifact-autoreact-ledger
  4575 mode             1767 agent-name            11 artifact-comment-monitor
  3376 custom-title     1716 started                9 launched
  3127 atis-latch       1586 result                 3 continued-in
                         998 bridge-session         1 fork-context-ref
                         876 file-history-delta
                         864 file-history-snapshot
                         749 pr-link
```

No `summary`, `progress`, `compact_boundary`, or `isCompactSummary` lines exist in
this corpus. Whatever compaction writes, it is not visible in these 30 days.

## 2. Every top-level type

Timestamp column: `ts` = has a top-level ISO `timestamp`; `—` = none (the line can only
be dated by file position or mtime).

| type | ts | key set (beyond `type`,`sessionId`) | example (scrubbed) | fact for a daemon |
|---|---|---|---|---|
| `user` | ts | `uuid,parentUuid,timestamp,cwd,gitBranch,version,entrypoint,userType,isSidechain,message,promptId` + optional `origin,promptSource,permissionMode,toolUseResult,toolDenialKind,sourceToolAssistantUUID,sourceToolUseID,toolEndsTurn,turnCompanion,isMeta,interruptedMessageId,classifierMetaLines,queueSkipAttachments,queuePriority,imagePasteIds,agentId,slug,sessionKind` | see §5 | prompt submitted, tool result delivered, denial, interrupt |
| `assistant` | ts | same envelope + `requestId,apiBlockIndex,effort,perTurnEffort,attributionAgent,attributionSkill,attributionPlugin,attributionMcpServer,attributionMcpTool,agentId,slug,session_id` + on errors `isApiErrorMessage,error,apiErrorStatus,quotaLimits` | see §4, §6 | model produced a block; call ids; stop reason; usage; API errors |
| `system` | ts | `uuid,parentUuid,subtype,timestamp,cwd,…` + per-subtype fields | see §3 | turn end, turn duration, hook outcome, usage-limit pause, agents killed, scheduled fire |
| `attachment` | ts | envelope + `attachment:{type,…}` + optional `rendered,renderedInHumanTurn` | `{"attachment":{"type":"queued_command","commandMode":"task-notification","timestamp":"…"}}` | context injected by the client; hook failures; plan/auto mode changes; queued commands |
| `last-prompt` | — | `lastPrompt,leafUuid` | `{"type":"last-prompt","lastPrompt":"<str:60>","leafUuid":"cb01…"}` | the leaf uuid of the chain; rewritten every turn |
| `mode` | — | `mode` | `{"type":"mode","mode":"normal"}` | only value seen: `normal` |
| `permission-mode` | — | `permissionMode` | `{"type":"permission-mode","permissionMode":"auto"}` | values: `auto`,`default`,`plan`,`acceptEdits` |
| `atis-latch` | — | `atis` | `{"type":"atis-latch","atis":""}` | always empty string in corpus |
| `bridge-session` | — | `bridgeSessionId,lastSequenceNum,ownerAccountUuid,ownerOrganizationUuid` | `{"bridgeSessionId":"cse_01…","lastSequenceNum":0,…}` | session is bridged to claude.ai (Remote Control) |
| `ai-title` / `custom-title` | — | `aiTitle` / `customTitle` | `{"type":"ai-title","aiTitle":"<str:33>"}` | display name for the session (custom wins) |
| `agent-name` | — | `agentName` | `{"type":"agent-name","agentName":"<str:29>"}` | the session's self-chosen name (same as `custom-title` here) |
| `queue-operation` | ts | `operation,timestamp` + `content` (enqueue/remove) + `reason` (remove) | `{"operation":"remove","reason":"absorbed_mid_turn",…}` | user typed while the agent was busy; `enqueue`→`dequeue`/`remove`; reasons `absorbed_mid_turn`,`session_start`,`delivered_to_agent` |
| `file-history-snapshot` | inner | `messageId,snapshot{messageId,trackedFileBackups,timestamp},isSnapshotUpdate` | | checkpoint taken at each prompt; precedes the human `user` line |
| `file-history-delta` | ts | `messageId,snapshotMessageId,trackingPath,backup{backupFileName,version,backupTime,realParentDir},timestamp` | | a tracked file was edited (Write/Edit) |
| `pr-link` | ts | `prNumber,prUrl,prRepository,timestamp` | | session created a PR |
| `frame-link` | ts | `artifactCount,timestamp` + `path,frameUrl,title` | | session published an artifact |
| `cost-state` | epoch | `totalCostUSD,totalAPIDuration,totalAPIDurationWithoutRetries,totalToolDuration,totalDuration,totalLinesAdded,totalLinesRemoved,startTime,hasUnknownModelCost,modelUsage{<model>:{inputTokens,outputTokens,thinkingTokens,cacheReadInputTokens,cacheCreationInputTokens,webSearchRequests,costUSD}}` | | session totals; written as one of the final lines at session end (84 of 370 sessions) |
| `continued-in` | ts | `continuedInSessionId,timestamp` | | this session id was superseded (resume/continue); follow the pointer |
| `fork-context-ref` | — | `agentId,parentSessionId,parentLastUuid,contextLength` | | a `fork` subagent's context anchor |
| `artifact-comment-monitor` / `artifact-autoreact-ledger` | — | `v,artifacts{<id>:{state,writtenAtMs,…}}` | `"state":"armed"` | artifact watch state; not agent activity |
| `launched` / `started` / `result` / `failed` | — | journal only: `key,agentId,label,phase` / `result{ok,originMain,output,tasks[]}` | | workflow bookkeeping, lives in `journal.jsonl` |

Metadata lines (`last-prompt`, `mode`, `permission-mode`, `atis-latch`,
`bridge-session`, `ai-title`, `custom-title`, `agent-name`) occur only in root
transcripts, never in `subagents/`. They are re-emitted as a block after every
prompt submission and often at turn end, so their presence does not date anything.

## 3. `system` lines

2,308 lines. Every one carries `timestamp`, `uuid`, `parentUuid`, `subtype`. `level` is
present only on some subtypes.

| subtype | count | level | extra keys | scrubbed example | fact |
|---|---|---|---|---|---|
| `stop_hook_summary` | 1,138 | `suggestion` | `hookCount,hookInfos[{command,durationMs}\|{command,promptText}],hookErrors[],hookAdditionalContext[],preventedContinuation,stopReason,hasOutput,toolUseID` | `{"subtype":"stop_hook_summary","hookCount":2,"hookInfos":[{"command":"<str:60>","durationMs":39},…],"preventedContinuation":false,"stopReason":"","hasOutput":false}` | **the turn ended and Stop hooks ran**; lands 74–116 ms after the final assistant block. `preventedContinuation:true` never seen; `hasOutput:true` twice (prompt-based goal hook) |
| `turn_duration` | 720 | — | `durationMs,messageCount` + optional `pendingWorkflowCount,pendingBackgroundAgentCount` | `{"subtype":"turn_duration","durationMs":33205,"messageCount":59}` | wall time of the whole turn (prompt→stop); 5 ms after `stop_hook_summary`; present in 116 of 148 human sessions |
| `local_command` | 219 | `info` | `content` | `{"subtype":"local_command","content":"<str:60>","level":"info"}` | user ran a `/` command; already used by Go |
| `away_summary` | 170 | — | `content` | `{"subtype":"away_summary","content":"<str:60>"}` | written when the user comes back after an idle gap (3–5 min after `turn_duration` in traces); marks "user returned", not agent activity |
| `scheduled_task_fire` | 26 | — | `content,cron,prompt,taskId` | content opens `Running scheduled task (Aug 27 4:23am)` | a `/loop` or cron task fired: agent activity with no human present |
| `agents_killed` | 18 | — | (none) | `{"subtype":"agents_killed"}` | the kill-all gesture (Ctrl+C twice / Esc); every child is dead |
| `informational` | 17 | `notice`(10) / `warning`(7) | `content` | notice: `Usage limit reached · continuing automatically at 5:50pm`, `Usage limit reset · continuing automatically`; warning: `Backgrounding after the current tool finishes…`, `Remote Control not started here · another Claude Code on thi…` | **usage-limit pause and resume**, backgrounding, remote-control refusal |
| `bridge_status` | 1 | — | `content,url` | | Remote Control bridge came up |

The `stopReason` inside `stop_hook_summary` is always `""` in the corpus; do not
confuse it with `message.stop_reason`.

## 4. Message content blocks and stop reasons

Block-type combinations per line (sample of 70 files):

```
2887 user      tool_result            341 assistant text        (stop tool_use)
2429 assistant tool_use (stop tool_use) 219 user      STRING
 826 assistant thinking (stop tool_use) 118 assistant text        (stop end_turn)
 557 assistant thinking (stop null)      92 assistant text        (stop null)
 457 assistant tool_use (stop null)      63 assistant thinking    (stop end_turn)
  18 user      text                       3 user      text+image
   3 assistant text     (stop stop_sequence)  1 assistant tool_use (stop end_turn)
```

One assistant line = one content block. Blocks seen: `thinking`, `text`, `tool_use`
(assistant); `tool_result`, `text`, `image` (user). `tool_result.content` is a string or
an array of `text`/`image` blocks. Corpus-wide `stop_reason` values:

```
72748 "tool_use"    2345 "end_turn"    176 "stop_sequence"    2 "refusal"    61690 null
```

`stop_sequence` is almost entirely the synthetic API-error line (§6): 165 of 176.
No `max_tokens`, `pause_turn`, or `stuck_on_*` values exist.

**Where `stop_reason` is populated differs by file kind.** Sequence of `stop_reason`
over the lines of one `requestId`:

| root transcripts (sample) | subagent transcripts (sample) |
|---|---|
| `tool_use,tool_use` ×454; `tool_use` ×431; `end_turn,end_turn` ×63; `end_turn` ×49 | `null,tool_use` ×384; `tool_use` ×243; `null,null,tool_use` ×104; `null,null` ×40; `null` ×19 |

In root files every block line already carries the final stop reason, including a
`thinking` block written 32 s before the closing `text` block (trace in §7). In subagent
files earlier blocks carry `null` and only the last block carries the value; requests
ending in `null` are agents that were killed or are still running. Rule: **a
non-null `stop_reason` says how the request will end, not that it has ended.**

## 5. Classifying `user` lines

Enum fields seen corpus-wide:

```
origin.kind    human 850 · task-notification 348 · peer 65 · coordinator 7 · auto-continuation 3
promptSource   typed 637 · system 410 · sdk 207 · queued 39 · suggestion_accepted 8
userType       external 271108 (the only value)
isMeta         true 1241 · false 1171 (absent otherwise)
toolDenialKind permission-rule 273 · automode-blocked 67 · user-rejected 21 · automode-unavailable 4 · interrupted 1
toolEndsTurn   true 1514 (only on StructuredOutput results in the sample)
turnCompanion  true 123 (isMeta lines that ride along with a human turn: skill text, <command-message>)
isCompactSummary   never present
```

`toolUseResult` forms: object (Bash: `interrupted,isImage,noOutputExpected,stderr,stdout`
+ optional `backgroundTaskId,persistedOutputPath,returnCodeInterpretation,gitOperation`;
Edit: `filePath,oldString,newString,structuredPatch,userModified,…`; Read: `file,type`;
Agent: `agent_id,agent_type,model,name,status,team_name,…`; AskUserQuestion:
`annotations,answers,questions`) or a string on failure (`Error: Exit code 1`,
`Error: Permission to use Bash with command …`, `User rejected tool use`, `Error: This
agent is isolated in the worktree …`). Corpus-wide `tool_result.is_error`: 1,738 true,
65,131 false.

Origins of `user` lines whose content is a plain string (the ones that look like prompts):

| shape | count | what it is | human? |
|---|---|---|---|
| `origin.kind=human, promptSource=typed` | 578 | typed at the keyboard | **yes** |
| `origin.kind=human, promptSource=queued` | 36 | typed while busy, delivered at turn end | yes (typed earlier) |
| `origin.kind=human, promptSource=suggestion_accepted` | 8 | accepted a suggested prompt | yes |
| `origin.kind=human, content` opens `<command-message>` | 9 | slash command with a prompt body | yes |
| `origin.kind=task-notification, promptSource=system`, opens `<task-notification>` | 321 (+18 plain) | **a background task/agent finished; the client woke the model** | no |
| `origin.kind=peer, promptSource=system, isMeta=true` | 43 | teammate message delivered to a root session; `origin` also has `from,senderTaskId,name,body` | no |
| `origin.kind=coordinator, isMeta=true, isSidechain=true` | 7 | lead → teammate instruction | no |
| `origin.kind=auto-continuation, promptSource=system` | 3 | client auto-continued a stopped turn | no |
| `isSidechain=true`, no origin, opens `<teammate-message` | 216 | teammate message inside a subagent file | no |
| `isSidechain=true`, no origin, plain | 1,762 | the subagent's launch prompt (first line of every subagent file, `parentUuid:null`) | no |
| `isMeta=true`, opens `<system-reminder>` | 765+ | injected reminder | no |
| `isMeta=true`, opens `<local-command-caveat>` | 106 | caveat after a `!` command | no |
| no origin, opens `<command-name>` / `<local-command-stdout>` / `<bash-input>` / `<bash-stdout>` | ~170 | `/clear`, `/model`, `!cmd` echo | no (local) |
| `promptSource=sdk`, no origin | 189 (+15 `[SYSTEM]`) | workflow / SDK-driven prompt | no |
| no origin, no promptSource, plain, `isSidechain=false` | 167 | bg sessions, older lines | unknown → treat as prompt, not as human presence |
| `isMeta=true`, opens `[Image:` | ~100 | pasted image placeholder | rides with a human turn |

Array-content `user` lines: `tool_result` (the bulk), `[Request interrupted by user]`
text (65 with `interruptedMessageId`, 28 older without; variant `…for tool use` ×27),
`text+image` human prompts (`origin.kind=human`, 27+3), and `isMeta` text blocks with
`sourceToolUseID` (skill or tool output re-injected as a user turn, 49).

Task-notification tag vocabulary (all structural): `<task-notification>`, `<task-id>`,
`<tool-use-id>`, `<status>` (`completed` 1,616 · `failed` 257 · `killed` 41 · `stopped`
40 · `running` 5), `<summary>`, `<result>`, `<output-file>`, `<usage>`, `<event>`,
`<diagnostics>`, `<task-type>` (`artifact-auto-react`, `artifact-watch-lifecycle`).
The `<tool-use-id>` is the `Agent`/`Bash` call id, so a daemon can close the
subagent it opened on the `Async agent launched` ack.

Each task notification is preceded (same ms) by an `attachment` of type
`queued_command` with `commandMode:"task-notification"`; a human queued prompt gets
`commandMode:"prompt"` (183 vs 173).

## 6. Error and limit signals

The only machine-readable error carrier is a **synthetic assistant line**:

```json
{"type":"assistant","isApiErrorMessage":true,"error":"rate_limit","apiErrorStatus":429,
 "quotaLimits":{"status":"rejected","resetsAt":1788994800,"rateLimitType":"five_hour",
   "overageStatus":"rejected","overageDisabledReason":"out_of_credits","isUsingOverage":false,
   "unifiedRateLimitFallbackAvailable":false},
 "requestId":"…","message":{"model":"<synthetic>","stop_reason":"stop_sequence",
   "usage":{"input_tokens":0,"output_tokens":0,…,"service_tier":null},
   "content":[{"type":"text","text":"You've hit your session limit · resets 4pm (…"}]}}
```

| `error` | `apiErrorStatus` | count | text opener (sentinel) | `quotaLimits` |
|---|---|---|---|---|
| `rate_limit` | 429 | 135 | `You've hit your session limit · resets …` / `You've hit your weekly limit · resets …` / `You're out of usage credits. Run /usage-…` | present on limit hits (`rateLimitType` `five_hour`/`seven_day`, `resetsAt` epoch s); absent on out-of-credits |
| `server_error` | 529 | 12 | `API Error: 529 Overloaded. This is a server-side issue…` | null |
| `server_error` | null | 13 | `API Error: The response stopped arriving. …` | null |
| `authentication_failed` | null | 5 | `Not logged in · Please run /login` | null |

Behaviour: while blocked, the client retries about every 5 min and writes one such line
per attempt (one session shows 15 lines over 60 min). Nothing follows the line except
metadata (`last-prompt`, …) until the retry succeeds or the user acts. The
`informational` system lines (§3) mark the pause/resume in newer versions:
`Usage limit reached · continuing automatically at …` (3) and `Usage limit reset ·
continuing automatically` (3). No `429`/`529` appears anywhere else; `retry`,
`stream`, `context window`, `rate limit` in prose are user/agent text, not signals
(206 `context window`, 623 `rate limit` hits are all inside content).

Tool-side errors: `tool_result.is_error:true` + `toolUseResult` string; hook failures
arrive as `attachment` lines `hook_non_blocking_error` (`hookName,hookEvent,exitCode,
stderr,durationMs,toolUseID`, 13 seen: `switchboard-ctl` missing) and `hook_cancelled`
(`timedOut:true,timeoutMs:2000`, 83 seen). Both are hook-health facts the daemon should
surface about itself.

## 7. Turn structure on disk

Trace of this session's root transcript (structural fields only):

```
 397 06:53:38.405 user        STR          origin=human promptSource=typed
 398 06:53:38.404 attachment  total_tokens_reminder
 399/400        file-history-snapshot ×2            (no timestamp)
 401 06:53:50.883 user        STR          origin=human   parentUuid JUMP (re-submitted prompt)
 402 06:53:50.883 attachment  total_tokens_reminder
 403 06:53:53.611 assistant   thinking     sr=end_turn rid=AAxvTa apiBlockIndex=0
 404 06:54:25.849 assistant   text         sr=end_turn rid=AAxvTa apiBlockIndex=1
 405 06:54:25.965 system      stop_hook_summary
 406/407        bridge-session, file-history-snapshot
 408 15:38:24.953 user        STR          origin=human
 409-412        attachment ×4 (total_tokens_reminder, instructions, session_context, date)
 413 15:38:32.494 assistant   thinking     sr=end_turn rid=jWgQsY apiBlockIndex=0
 414 15:38:59.745 assistant   text         sr=end_turn rid=jWgQsY apiBlockIndex=1
 415 15:38:59.850 system      stop_hook_summary
 416 15:38:59.855 system      turn_duration durationMs=34877 messageCount=256
 417            file-history-snapshot
 418 15:49:27.167 user        STR          origin=human
 420 15:49:57.773 assistant   thinking     sr=tool_use rid=FS8JMf apiBlockIndex=0
 422 15:50:02.885 assistant   tool_use     sr=tool_use rid=FS8JMf apiBlockIndex=2
 423 15:50:04.730 user        tool_result
 424 15:50:07.149 assistant   tool_use     rid=FS8JMf apiBlockIndex=3
 …   (13 tool_use blocks of one request, each followed by its tool_result)
 450 15:50:55.014 assistant   tool_use     rid=FS8JMf apiBlockIndex=12
 451-458        last-prompt, custom-title, ai-title, agent-name, mode, permission-mode, atis-latch, bridge-session
 459 15:50:56.279 user        tool_result
 462 15:51:18.322 assistant   thinking     sr=tool_use rid=AoeS8S apiBlockIndex=0
```

Facts:

- **A turn begins** with a `user` line (`origin.kind`/`promptSource` say who), written
  at submit time, immediately followed by `attachment` lines with the same or earlier
  millisecond. A `file-history-snapshot` (no timestamp) usually precedes it. The
  metadata block (`last-prompt` … `bridge-session`) is appended around the same time
  and again at turn end; it is not a turn marker.
- **`parentUuid` chains** every timestamped line (`user`, `assistant`, `attachment`,
  `system`) to the previous one. Breaks (`JUMP`) occur on re-submitted prompts and
  when parallel tool results land out of order (lines 446/448). Untimestamped metadata
  lines have no `parentUuid`. `last-prompt.leafUuid` names the current leaf.
- **`requestId` groups** the block lines of one API response; `apiBlockIndex` orders
  them; `message.id` is the same across them. Block lines are written as the client
  processes each block: a `thinking` block lands seconds before the `text` block that
  follows it, and `tool_use` blocks of a single request are interleaved with their
  `tool_result`s (client executes sequentially and emits the next block afterwards).
- **The first block arrives 3–30 s after the prompt** (long thinking); a daemon that
  wants "started" needs the UserPromptSubmit hook, or must treat "prompt on disk, no
  assistant line yet" as active.
- **A turn ends** with `system/stop_hook_summary` (74–116 ms after the last block),
  then `turn_duration` (+5 ms) when present. The last assistant line alone is *not*
  sufficient: in root files `stop_reason:"end_turn"` is already on the thinking block
  32 s before the final text lands in that corpus. **Measured against hooks on
  2026-09-15: `Stop` followed the transcript's last `end_turn` by a median of 60 ms
  and at most 101 ms across 30 pairs; the 32 s early close did not reproduce.** Rule: turn end = `stop_hook_summary`, else
  `turn_duration`, else `end_turn` text block with no newer line after a grace period.
  `stop_hook_summary` is missing from 10 of 148 human sessions (older versions, or
  sessions ended by `cost-state`); `turn_duration` from 32.
- **`end_turn` vs `tool_use`** partitions requests, not turns: a turn is one or more
  requests; all but the last end `tool_use` (or a `stop_sequence` error), the last ends
  `end_turn`. A request ending `tool_use` whose `tool_result` has not arrived is
  "tool running **or** awaiting permission"; the transcript cannot tell these apart
  (nothing is written until the decision). Only the PermissionRequest hook can.
- **AskUserQuestion** is a `tool_use` whose result (`toolUseResult{questions,answers,
  annotations}`) arrives when the human answers; between the two the session is
  waiting on the human. Same for `ExitPlanMode`. Tool names seen (sample):
  Bash 2275, Edit 254, Read 248, Write 86, WebFetch 35, StructuredOutput 28, Agent 19,
  TaskUpdate 10, ToolSearch 9, Workflow 5, Skill 5, AskUserQuestion 5, TaskCreate 4,
  WebSearch 3, Artifact 3, SendMessage 1.
- **Subagents** write only to their own file; the root file is untouched while a
  child works (the root here ended at 16:45:56 and stayed at that mtime through 5 min
  of child activity). A finished child ends with `assistant text sr=end_turn`
  (sample of 26 finished files: 4 `end_turn`, 15 `tool_use`, 6 `null`, 1
  `stop_sequence` as the last assistant line, so "last line is `end_turn`" is
  unreliable); the reliable completion signal is the root's `task-notification`
  user line carrying `<tool-use-id>`.
- **Session end**: `cost-state` (84 sessions) or `continued-in` are terminal; most
  sessions simply stop. There is no `session_end` line; death is a pidfd fact.
- **Idle gap markers**: `away_summary` appears 3–5 min after the last turn when the
  user returns; `scheduled_task_fire` starts a turn with no human.

## 8. Usage and cost

Every assistant line, including error lines, carries `message.usage`:

```
input_tokens, output_tokens, cache_read_input_tokens, cache_creation_input_tokens,
cache_creation{ephemeral_1h_input_tokens, ephemeral_5m_input_tokens},
output_tokens_details{thinking_tokens}, server_tool_use{web_search_requests, web_fetch_requests},
service_tier ("standard"), speed ("standard"), inference_geo ("not_available"),
iterations[{input_tokens, output_tokens, cache_*, type:"message"}]
```

`message` also has `id, model, role, stop_reason, stop_sequence, stop_details,
diagnostics` and sometimes `container, context_management` (all null in practice).
Per-line envelope has `effort` (`xhigh` 83k · `high` 38k · `medium` 10k · `low` 5k · older
`moderate/trivial/heavy/S/M/rewrite`) and `perTurnEffort`.

**Summing rule**: the usage object is identical on every block line of a request
(checked: 1 distinct usage per `requestId` across 1,240 requests). Sum once per
`requestId` (or `message.id`), never per line. Per-turn total = sum over requests
between two turn boundaries; per-session total = sum over the file plus its
`subagents/` files. `cost-state` gives the client's own session totals (USD, per-model
tokens, API and tool durations) but only at session end and only for 84 sessions, so it
is a check, not a source. `turn_duration.durationMs` is the wall time; the API-side
duration is not on disk.

## 9. Derivable facts

| fact | source line / field | confidence | latency to disk | Go uses today |
|---|---|---|---|---|
| a human typed a prompt | `user` + `origin.kind=="human"` (`promptSource` typed/queued/suggestion_accepted) | exact (v≥2.1.222) | 0 ms at submit | no (infers from any user text) |
| a prompt came from a background task, not a human | `user` + `origin.kind=="task-notification"`; `<status>` tag inside | exact | 0 ms | no |
| teammate / coordinator message | `origin.kind` peer/coordinator, or `<teammate-message` opener in sidechain | exact / heuristic | 0 ms | no |
| user typed while busy (queued) | `queue-operation` enqueue/dequeue/remove(+reason) | exact | 0 ms | no |
| model started responding | first `assistant` line of a new `requestId` | exact | 3–30 s after prompt (thinking) | yes (activity) |
| a tool call was issued | `assistant` block `tool_use{id,name}` | exact | ≈50 ms after `timestamp` (measured live) | yes |
| tool finished | `user` `tool_result{tool_use_id,is_error}`; `toolUseResult` shape | exact | 0 ms | yes |
| tool denied by human / rule / auto-mode | `toolDenialKind` (5 values) | exact | 0 ms | user-rejected only |
| waiting on human question | `tool_use.name=="AskUserQuestion"` with no result yet | exact | as above | no |
| awaiting permission vs tool running | not on disk | — | — | hook only |
| turn ended (Stop) | `system.subtype=="stop_hook_summary"` | exact when present | 74–116 ms after last block | no (hook) |
| turn wall time | `system.subtype=="turn_duration"` `.durationMs,.messageCount` | exact | +5 ms | no |
| request will end the turn | `stop_reason=="end_turn"` on any block | exact; early by 60 ms median, 101 ms max against the `Stop` hook (30 pairs, 2026-09-15). The 32 s figure came from block timestamps within one request | per block | yes |
| user interrupted | `[Request interrupted by user…]` text + `interruptedMessageId` | exact | 0 ms | yes (marker) |
| all agents killed | `system.subtype=="agents_killed"` | exact | 0 ms | no |
| usage-limit block | `assistant.isApiErrorMessage` + `error=="rate_limit"` + `quotaLimits.resetsAt` | exact | per retry (~5 min) | no |
| overloaded / stream dropped | `error=="server_error"` (`apiErrorStatus` 529 or null) | exact | per retry | no |
| logged out | `error=="authentication_failed"` | exact | once | no |
| auto-continue after limit | `system.informational` notice text opener | heuristic (prose) | 0 ms | no |
| Stop hook health | `stop_hook_summary.hookErrors`, `attachment.hook_non_blocking_error`, `hook_cancelled` | exact | 0 ms | no |
| subagent spawned | `tool_use.name=="Agent"` + ack result; new file in `subagents/` | exact | file create | yes |
| subagent finished | root `task-notification` with `<tool-use-id>`/`<status>` | exact | 0 ms | no (uses child file tail) |
| subagent identity / name | file name `agent-a<name>-<hex>`; `agentId`, `attributionAgent` (agent type) | exact | — | partial |
| session is a workflow/bg session | `sessionKind=="bg"`, `entrypoint=="sdk-cli"`, `promptSource=="sdk"` | exact | — | no |
| session display name | `custom-title` > `ai-title` > `agent-name` (last occurrence) | exact | untimestamped | no |
| permission mode | `permission-mode.permissionMode` (last occurrence); `user.permissionMode` per prompt | exact | untimestamped | no |
| session continued elsewhere | `continued-in.continuedInSessionId` | exact | 0 ms | no |
| session ended cleanly | `cost-state` present | heuristic (84/370) | at exit | no |
| user came back after idle | `system.subtype=="away_summary"` | exact | 3–5 min after turn | no |
| scheduled task ran | `system.subtype=="scheduled_task_fire"` | exact | 0 ms | no |
| PR / artifact produced | `pr-link`, `frame-link` | exact | 0 ms | no |
| file edited | `file-history-delta.trackingPath` (path only, no content) | exact | 0 ms | no |
| tokens per turn / session | `message.usage` deduped by `requestId` | exact | per block | yes (per line: over-counts) |
| cost in USD | `cost-state.totalCostUSD` | exact, end only | at exit | no |
| Claude Code version / model / effort | `version`, `message.model`, `effort` | exact | per line | no |

Latency notes: measured live in this session, an assistant `tool_use` line was on
disk 51 ms after its own `timestamp` and 12 ms before the tool started. The "flushed
~5 s later" claim in survey 03 §5.2 does not hold on 2.1.268; timestamps are write
times to within tens of milliseconds. Untimestamped metadata lines are the only
thing that moves mtime without a datable event.

## 10. Rust sketch

Only the fields worth reading. Everything else stays in `serde_json::Value` or is
skipped by `#[serde(default)]`.

```rust
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Line {
    User(UserLine),
    Assistant(AssistantLine),
    System(SystemLine),
    Attachment(AttachmentLine),
    QueueOperation { operation: QueueOp, timestamp: String },
    PermissionMode { #[serde(rename = "permissionMode")] mode: String },
    CustomTitle { #[serde(rename = "customTitle")] title: String },
    AiTitle { #[serde(rename = "aiTitle")] title: String },
    AgentName { #[serde(rename = "agentName")] name: String },
    ContinuedIn { #[serde(rename = "continuedInSessionId")] session: String },
    CostState { #[serde(rename = "totalCostUSD")] cost_usd: f64 },
    #[serde(other)]
    Other,               // mode, atis-latch, bridge-session, file-history-*, pr-link, …
}

#[derive(Deserialize)]
pub struct UserLine {
    pub timestamp: String,
    pub uuid: String,
    #[serde(default)] pub origin: Option<Origin>,          // kind: human | task-notification | peer | coordinator | auto-continuation
    #[serde(default, rename = "promptSource")] pub prompt_source: Option<String>,
    #[serde(default, rename = "isMeta")] pub is_meta: bool,
    #[serde(default, rename = "isSidechain")] pub is_sidechain: bool,
    #[serde(default, rename = "toolDenialKind")] pub tool_denial_kind: Option<String>,
    #[serde(default, rename = "interruptedMessageId")] pub interrupted_message_id: Option<String>,
    #[serde(default, rename = "toolUseResult")] pub tool_use_result: Option<serde_json::Value>, // string ⇒ error
    pub message: Message,
}

#[derive(Deserialize)] pub struct Origin { pub kind: String }

#[derive(Deserialize)]
pub struct AssistantLine {
    pub timestamp: String,
    pub uuid: String,
    #[serde(default, rename = "requestId")] pub request_id: Option<String>,
    #[serde(default, rename = "apiBlockIndex")] pub api_block_index: Option<u32>,
    #[serde(default, rename = "isApiErrorMessage")] pub is_api_error: bool,
    #[serde(default)] pub error: Option<String>,             // rate_limit | server_error | authentication_failed
    #[serde(default, rename = "apiErrorStatus")] pub api_error_status: Option<u16>,
    #[serde(default, rename = "quotaLimits")] pub quota_limits: Option<QuotaLimits>,
    pub message: Message,
}

#[derive(Deserialize)]
pub struct QuotaLimits { #[serde(rename = "rateLimitType")] pub kind: Option<String>, #[serde(rename = "resetsAt")] pub resets_at: Option<u64> }

#[derive(Deserialize)]
pub struct Message {
    #[serde(default)] pub id: Option<String>,
    #[serde(default)] pub model: Option<String>,
    #[serde(default)] pub stop_reason: Option<String>,        // tool_use | end_turn | stop_sequence | refusal
    #[serde(default)] pub usage: Option<Usage>,
    #[serde(default)] pub content: Content,
}

#[derive(Deserialize, Default)]
#[serde(untagged)]
pub enum Content { Text(String), Blocks(Vec<Block>), #[default] Empty }

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Block {
    ToolUse { id: String, name: String },                 // input skipped on purpose (privacy)
    ToolResult { tool_use_id: String, #[serde(default)] is_error: bool },
    Text { text: String },                                // read only for the interrupt sentinel and openers
    Thinking {}, Image {},
    #[serde(other)] Other,
}

#[derive(Deserialize, Clone, Copy, Default)]
pub struct Usage {
    #[serde(default)] pub input_tokens: u64,
    #[serde(default)] pub output_tokens: u64,
    #[serde(default)] pub cache_read_input_tokens: u64,
    #[serde(default)] pub cache_creation_input_tokens: u64,
}

#[derive(Deserialize)]
pub struct SystemLine {
    pub timestamp: String,
    pub subtype: String,   // stop_hook_summary | turn_duration | local_command | away_summary | scheduled_task_fire | agents_killed | informational | bridge_status
    #[serde(default)] pub level: Option<String>,
    #[serde(default, rename = "durationMs")] pub duration_ms: Option<u64>,
    #[serde(default, rename = "hookErrors")] pub hook_errors: Vec<serde_json::Value>,
    #[serde(default)] pub content: Option<String>,   // only inspected for the two usage-limit openers
}

#[derive(Deserialize)]
pub struct AttachmentLine { pub timestamp: String, pub attachment: Attachment }
#[derive(Deserialize)]
pub struct Attachment { #[serde(rename = "type")] pub kind: String, #[serde(default, rename = "commandMode")] pub command_mode: Option<String>, #[serde(default, rename = "hookName")] pub hook_name: Option<String> }

#[derive(Deserialize, Clone, Copy)] #[serde(rename_all = "lowercase")]
pub enum QueueOp { Enqueue, Dequeue, Remove }
```

Tail reader. One per open transcript (root and each `subagents/*.jsonl`); the
`subagents/` directory is polled for new files on the same tick as `/proc`.

```rust
pub struct Tail {
    path: PathBuf,
    file: File,
    cursor: u64,          // byte offset of the first unconsumed byte
    partial: Vec<u8>,     // bytes after the last '\n' (a line still being written)
    last_len: u64,        // detect truncation/rotation: len < cursor ⇒ reopen from 0
}

const WINDOW: u64 = 4 << 20;   // on first open, seek to max(0, len - WINDOW) and drop the first partial line

impl Tail {
    pub fn poll(&mut self, sink: &mut impl FnMut(Line)) -> io::Result<()> {
        let len = self.file.metadata()?.len();
        if len < self.cursor { self.reopen()?; }
        if len == self.cursor { return Ok(()); }
        let mut buf = vec![0; (len - self.cursor).min(1 << 20) as usize];
        let n = self.file.read_at(&mut buf, self.cursor)?;   // never read past len: a 20 MB file is normal
        self.cursor += n as u64;
        self.partial.extend_from_slice(&buf[..n]);
        while let Some(i) = memchr::memchr(b'\n', &self.partial) {
            let line = self.partial.drain(..=i).collect::<Vec<u8>>();
            if line.len() > 1 {                                // lines up to hundreds of KB exist; bound at 16 MB
                match serde_json::from_slice::<Line>(&line) { Ok(l) => sink(l), Err(_) => {} } // unknown shapes are not errors
            }
        }
        Ok(())
    }
}
```

Reducer inputs the tail should emit (one enum, no content):

```rust
pub enum Ev {
    Prompt { at: Ts, human: bool },                 // user + (origin human | typed/queued/suggestion_accepted)
    Wake { at: Ts, kind: WakeKind },                // task-notification | peer | coordinator | auto-continuation | scheduled_task_fire
    Block { at: Ts, req: String, stop: Option<Stop>, usage: Usage },
    ToolCall { at: Ts, id: String, name: String, req: String },
    ToolDone { at: Ts, id: String, error: bool, denial: Option<String> },
    Interrupted { at: Ts },
    TurnEnd { at: Ts, hook_errors: usize },         // stop_hook_summary (turn_duration as fallback)
    ApiError { at: Ts, kind: ApiErrorKind, resets_at: Option<u64> },
    AgentsKilled { at: Ts },
    Queue { at: Ts, op: QueueOp },
    Meta(MetaChange),                               // title, permission mode, continued-in, cost-state
}
```

Rules the reducer must keep from this survey: dedupe `usage` by `req`; do not end a
turn on `stop == end_turn` alone; a `ToolCall` without `ToolDone` is "running or
awaiting permission" and only a hook can split that; a `Wake` is agent activity, never
human presence; an `ApiError{rate_limit}` with a future `resets_at` is a distinct
"blocked" state that should paint neither red nor green.
