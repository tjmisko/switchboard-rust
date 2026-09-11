# 14 — Hook events verified against the installed binaries (2026-09-11)

Source of truth for this document: `rg -a` over the installed Claude Code
binary `~/.local/share/claude/versions/2.1.268` (ELF aarch64, 218 MB) and the
installed Codex binary
`~/.codex/packages/standalone/releases/0.154.0-aarch64-unknown-linux-musl/bin/codex`.
The Claude field sets below are the literal emitter objects found in the
bundle (minified, so spread names like `Ia(...)` and `oe` are the common base
builder; the named keys are exact). The Go repo's hook schema doc was verified
against 2.1.222; this supersedes it for the events it covers.

The Go daemon wires eight Claude events and eight Codex events. Both agents
now emit substantially more.

## 1. Claude Code 2.1.268 — every event has an emitter

Presence check (`hook_event_name:"<Event>"` literal in the bundle, count):

| Event | emitter | Event | emitter |
|---|---|---|---|
| SessionStart | 1 | TaskCreated | 1 |
| UserPromptSubmit | 1 | TaskCompleted | 1 |
| PreToolUse | 1 | Stop | 1 |
| PermissionRequest | 1 | StopFailure | 1 |
| PermissionDenied | 1 | TeammateIdle | 1 |
| PostToolUse | 1 | CwdChanged | 1 |
| PostToolUseFailure | 1 | PreCompact | 1 |
| PostToolBatch | 1 | PostCompact | 1 |
| Notification | 1 | Elicitation | 1 |
| SubagentStart | 1 | ElicitationResult | 1 |
| SubagentStop | 1 | SessionEnd | 1 |
| MessageDisplay | 1 | | |

Not checked individually (present in the docs list the owner supplied):
Setup, UserPromptExpansion, InstructionsLoaded, ConfigChange, DirectoryAdded,
FileChanged, WorktreeCreate, WorktreeRemove, PreModelSwitch, PostModelSwitch.
None of these carries status information for the daemon.

### 1.1 Emitter field sets (verbatim keys from the bundle)

Every event spreads a common base first. From the Go schema doc (2.1.222) the
base is `session_id, transcript_path, cwd, prompt_id?, permission_mode,
agent_id? (subagents only), agent_type, effort`. The base builder could not be
re-extracted cleanly from 2.1.268 because of a minifier name collision; the
per-event keys below are exact.

| Event | Event-specific keys | Notes |
|---|---|---|
| `SessionStart` | `source, agent_type, model, session_title` | `session_title` is the session's display name (custom title, else generated) |
| `UserPromptSubmit` | `prompt, session_title` | plus `source` ∈ `user\|sdk\|system\|loop_wakeup\|schedule_wakeup\|poll_event` per the input schema (survey 12 §1.3); fires on machine wake-ups too |
| `PreToolUse` | `tool_name, tool_input, tool_use_id` | id on every tool; Go restricts the matcher to `AskUserQuestion\|ExitPlanMode` |
| `PermissionRequest` | `tool_name, tool_input, permission_suggestions` | **still no `tool_use_id`** |
| `PermissionDenied` | `tool_name, tool_input, tool_use_id, reason` | fires on auto-mode denials; whether a human decline at the prompt fires it is unmeasured |
| `PostToolUse` | `tool_name, tool_input, tool_response, tool_use_id, duration_ms` | `tool_input` is the post-approval rewritten input |
| `PostToolUseFailure` | `tool_name, tool_input, tool_use_id, error, is_interrupt, duration_ms` | exact-id release edge for a failed or interrupted tool |
| `PostToolBatch` | `tool_calls` | after a parallel batch resolves; not needed |
| `Notification` | `message, title, notification_type` | `notification_type` ∈ `permission_prompt, idle_prompt, elicitation_dialog, auth_success` and (survey 13) `quota_auto_resume_fired, quota_auto_resume_stale, quota_auto_resume_disabled` |
| `Stop` | `stop_hook_active, last_assistant_message, …q` | the `…q` spread carries `background_tasks[]` and `session_crons[]` (field names present 23× and 3× in the bundle; survey 12 §0) |
| `StopFailure` | `error, error_details, last_assistant_message` | main thread only; fires on every `isApiErrorMessage` path; `Stop` does not (survey 13 §3) |
| `SubagentStart` | `agent_id, agent_type` | |
| `SubagentStop` | `stop_hook_active, agent_id, agent_transcript_path, agent_type, last_assistant_message, …q` | same `background_tasks` spread as Stop |
| `TeammateIdle` | `teammate_name, team_name` | `team_name` documented deprecated |
| `TaskCreated` / `TaskCompleted` | `task_id, task_subject, task_description, teammate_name, team_name` | |
| `CwdChanged` | `old_cwd, new_cwd` | |
| `PreCompact` | `trigger, custom_instructions` | `trigger` ∈ `manual\|auto` |
| `Elicitation` | `mcp_server_name, message, mode, url, elicitation_id, requested_schema` | MCP asking the human: an attention edge invisible today |
| `MessageDisplay` | `turn_id, message_id, index, final, delta` | **per streamed delta; never wire** |
| `SessionEnd` | `reason` | not liveness; a crash fires nothing |

`PermissionDenied.reason` values seen in the bundle (partial): `abandoned,
aborted, aborted_streaming, aborted_tools, absent, absorbed_mid_turn,
account_boundary, additional_properties, …` (the string table is shared with
other subsystems; treat `reason` as an open string).

### 1.2 What the richer set closes (Claude)

| Event | Gap it closes in the Go design |
|---|---|
| `PreToolUse` unrestricted | The transcript call-id latch (survey 03 §2.4: five guards, 3 s skew grace, 2 s confirm grace, `Latch` states). A `PermissionRequest` joins to the writer's open `PreToolUse` of the same tool in one step. |
| `PermissionDenied` | Exact-id release for a denied call; today visible only as a transcript `toolDenialKind` line |
| `PostToolUseFailure` | Esc during a tool and tool errors by exact id; today only the interrupt marker |
| `StopFailure` | A turn that died on an API error; today a silent abort (chip stays green) |
| `Notification{idle_prompt}` | Candidate replacement for the lost pane-title H9 recovery |
| `Elicitation` | MCP user-input requests as attention |
| `SubagentStop{agent_transcript_path}` | Child completion at hook speed; the `subagents/` scan becomes backfill |
| `SessionStart{session_title}`, `UserPromptSubmit{session_title}` | Naming without reading `~/.claude/sessions/<pid>.json` or transcript title lines |
| `Stop{last_assistant_message}` | Same display text Codex naming already uses |
| `CwdChanged` | Go captures cwd once at discovery |
| `PreCompact`/`PostCompact` | A compaction reads as a long silent turn today |
| `Stop{background_tasks, session_crons}` | The whole "waiting on a monitor/timer" state (survey 12) |

Unchanged: `PermissionRequest` has no `tool_use_id`; hooks remain enrichment
(a SIGKILL fires nothing), so pidfd and the transcript-as-falsifier stay.

## 2. Codex 0.154 — twelve events

Presence check on the Codex binary (`"<Event>"` literal count) and the field
name strings present:

| Event | present | Field names present in binary |
|---|---|---|
| SessionStart | yes | `hook_event_name, permission_mode, transcript_path, agent_type, prompt, tool_input, last_assistant_message, agent_transcript_path, tool_use_id, turn_id, agent_id, end_reason, compaction_trigger` (all found; grouped by event only where documented) |
| UserPromptSubmit | yes | `prompt` (bounded to 1,000 chars by the ctl) |
| PreToolUse | yes | `tool_name, tool_input, tool_use_id, turn_id, source, permission_mode` (survey 09 §2.1 fixture) |
| PermissionRequest | yes | `tool_name`, matcher by tool name incl. `Bash`, `apply_patch*`, MCP tools |
| PostToolUse | yes | `tool_use_id, turn_id` |
| Stop | yes | `last_assistant_message` |
| SubagentStart / SubagentStop | yes | `agent_id` (child thread id), `agent_type`, `turn_id`, `agent_transcript_path` |
| **Interrupt** | yes | none beyond base; main thread only ("doesn't run for subagents") |
| **PreCompact / PostCompact** | yes | `compaction_trigger` ∈ `manual\|auto` |
| **SessionEnd** | yes | `end_reason` (currently only `other`); main thread only |

Docs supplied by the owner (matcher column): `UserPromptSubmit`, `Stop`,
`Interrupt` take no matcher; the others match on tool name, compaction
trigger, start source, end reason, or subagent type.

### 2.1 What the three new Codex events close

| Event | Closes |
|---|---|
| `Interrupt` | The release edge Codex never had. The Go hook reducer opens a 30 s grace on a generic `PermissionRequest` and re-asserts red until `Stop`, `SessionStart`, or the next `UserPromptSubmit`; Ctrl-C at an approval prompt stays red until you type. `Interrupt` ends the turn and clears every pending record (root only, writer-blind, consistent with D-35). Retires FR-E8b's `turn_aborted` inference. |
| `PreCompact`/`PostCompact` | Busy, not idle, across a compaction |
| `SessionEnd` | Clean-exit reason for history; not liveness |

### 2.2 What Codex still cannot tell us

| Gap | Claude | Codex |
|---|---|---|
| exact id on a denied/failed tool | `PermissionDenied`, `PostToolUseFailure` | nothing; denied gate released only by `Interrupt`, `Stop`, next prompt |
| the UI is waiting on the human | `Notification`, `Elicitation` | nothing; `request_user_input` on `PreToolUse` is the one immediate red |
| who owns an approval gate | not needed | still the app-server's `approvalsReviewer` or the 30 s grace |
| cwd change | `CwdChanged` | nothing |
| turn died on API error | `StopFailure` | nothing |

## 3. Recommended v1 hook set

**Claude** (wire all, one command `switchboard-ctl hook <Event>`):
SessionStart, UserPromptSubmit, PreToolUse (no matcher), PermissionRequest,
PermissionDenied, PostToolUse, PostToolUseFailure, Stop, StopFailure,
Notification, Elicitation, ElicitationResult, SubagentStart, SubagentStop,
TeammateIdle, CwdChanged, PreCompact, PostCompact, SessionEnd.
Never `MessageDisplay`, `PostToolBatch`.

**Codex** (`switchboard-ctl codex-hook <Event>`): all twelve.

Cost: each hook is a process spawn with a 2 s budget. `PostToolUse` already
fires per tool call; unrestricted `PreToolUse` doubles that. Nothing else is
high-frequency. The forwarder must stay runtime-free and exit in milliseconds.

**Privacy boundary in the ctl** (unchanged principle, wider surface): forward
ids, names, enums, timestamps, `background_tasks[].{id,type,status}`,
`session_crons[].{id,schedule,recurring}`; hash `tool_input`; drop
`tool_response`, `prompt` (except the bounded Codex naming edges),
`last_assistant_message` (same), `message`/`title` on Notification except
`notification_type`, `requested_schema`, `command`, `description`.

## 4. Vocabulary consequence

Eleven events are common to both agents: session start/end, prompt, tool
start with id, permission asked, tool end with id, stop, interrupt, compact,
subagent start/stop. Claude adds six the core can carry as optional variants.
Model `HookKind` as one provider-neutral enum with a Claude-only extension,
not two vocabularies. Keep the two *reducers* separate until the Codex
writer-blind sweeps are understood (D-35 came from a real incident).

## 5. Unmeasured behaviours (settled by a capture harness)

1. Does `PermissionDenied` fire when a human declines at the prompt?
2. Does `Stop` fire on an Esc interrupt in 2.1.268?
3. When does `Notification{idle_prompt}` fire relative to the turn end?
4. Does a pre-first-token double-Esc emit any event?
5. Is `UserPromptSubmit.source` `system` on a task-notification wake?
6. Does `Stop.background_tasks` list a persistent Monitor?
7. Does `Stop` fire at all when the harness reaps a background Bash at the final response?
8. Does `SubagentStop` fire for workflow agents and for teammate shutdown?
9. Does `TeammateIdle` fire for a teammate that stops on an error?
10. Is the hook `agent_id` for a teammate the file stem or `<name>@<team>`?
11. Does `Notification{quota_auto_resume_fired}` fire on 2.1.268?
12. Does `StopFailure` fire for a `-p` headless root?
13. Codex: does `Interrupt` fire when Ctrl-C lands during an approval prompt (vs. only mid-generation)?

Harness shape: one script, `switchboard-hookcap <Event>`, that appends
`{"ts", "agent", "event", "payload"}` (payload verbatim, this is a local
measurement file) to `~/.local/state/switchboard/hookcap/<day>.jsonl`, wired
for every event above in `~/.claude/settings.json` and `~/.codex/hooks.json`
alongside the existing `switchboard-ctl` hooks, run for a working day, then
removed. The Rust `HookKind` enum is then designed from the recording.
