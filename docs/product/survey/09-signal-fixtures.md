# 09 — Signal fixtures: verbatim input shapes the Go daemon consumes

Every payload below is copied from the Go repo at `~/Projects/switchboard`
(docs, `testdata/`, or `_test.go` string literals). Citations are `path:line`.
Where the repo has no literal fixture, the shape is reconstructed from the Go
struct tags and marked **(reconstructed)**. String values are trimmed in places;
keys are never dropped.

---

## 1. Claude Code hook stdin payloads

**Transport:** JSON on stdin of `switchboard-ctl hook <Event>`, one process per
event, spawned by Claude Code from the `hooks` block of `settings.json`
(`README.md:351-359`). The ctl reads it with `io.ReadAll(os.Stdin)`
(`cmd/switchboard-ctl/main.go:630`), parses `hookPayload`
(`main.go:642-659`), hashes `tool_input`, and forwards an `rpc.Request` over the
Unix socket (`main.go:665-691`). Cadence: one per event; PostToolUse fires for
every tool call in every session. Fields the daemon actually reads are the ones
in `hookPayload`: `session_id`, `transcript_path`, `tool_name`, `tool_input`
(hash only), `agent_id`/`agentId`, `agent_type`/`agentType`, plus the
Codex-only `prompt`, `last_assistant_message`, `source`, `turn_id`,
`tool_use_id`, `permission_mode`. `prompt_id`, `cwd`, `permission_mode`
(for Claude), `effort`, `tool_response`, `duration_ms`,
`permission_suggestions` are on the wire but ignored.

### 1.1 Common base (every event) — binary-verified

From `docs/claude-code-hook-schema.md:20-33` (Claude Code 2.1.222 bundle):

```js
function $f(e,t,r){
  return {
    session_id:      n,
    transcript_path: HM(n),          // ALWAYS the parent session's .jsonl
    cwd:             Nt(),
    prompt_id:       XPt() ?? void 0, // absent until first user input
    permission_mode: e,               // default|acceptEdits|bypassPermissions|plan|dontAsk|auto
    agent_id:        r?.agentId,      // ONLY inside a subagent; absent on main thread
    agent_type:      o,               // subagent, OR main thread of an --agent session
    effort:          a,
  }
}
```

### 1.2 SessionStart

**(reconstructed)** from the base above; `source` is not read for Claude.

```json
{
  "session_id": "e0b4b21f-aaf6-4ab0-a8d6-2d595aba4065",
  "transcript_path": "/home/tjmisko/.claude/projects/-home-tjmisko-Projects-switchboard/e0b4b21f-aaf6-4ab0-a8d6-2d595aba4065.jsonl",
  "cwd": "/home/tjmisko/Projects/switchboard",
  "permission_mode": "default",
  "hook_event_name": "SessionStart",
  "source": "startup"
}
```

### 1.3 UserPromptSubmit

**(reconstructed)**; the ctl reads `prompt` only for Codex (`main.go:684`).

```json
{
  "session_id": "e0b4b21f-aaf6-4ab0-a8d6-2d595aba4065",
  "transcript_path": "/home/tjmisko/.claude/projects/.../e0b4b21f.jsonl",
  "cwd": "/home/tjmisko/Projects/switchboard",
  "prompt_id": "9b1c2d3e-...",
  "permission_mode": "default",
  "hook_event_name": "UserPromptSubmit",
  "prompt": "fix the flaky test"
}
```

### 1.4 PreToolUse — test fixture, subagent-fired (has `agent_id`, has `tool_use_id`)

`cmd/switchboard-ctl/hookinput_test.go:362-369`:

```json
{
  "session_id": "sess-1",
  "transcript_path": "/tmp/t.jsonl",
  "tool_name": "AskUserQuestion",
  "tool_input": {"questions":[{"question":"secret question"}]},
  "tool_use_id": "toolu_ask",
  "agent_id": "agent-child"
}
```

Emitter shape (`docs/claude-code-hook-schema.md:87`):
`{...$f(…), hook_event_name:"PreToolUse", tool_name, tool_input, tool_use_id}`.
Only registered for the `AskUserQuestion|ExitPlanMode` matcher (`README.md:354`).

### 1.5 PermissionRequest — test fixture (has `agent_id`, **no `tool_use_id`**)

`cmd/switchboard-ctl/hookinput_test.go:335-342`:

```json
{
  "session_id": "sess-1",
  "transcript_path": "/tmp/t.jsonl",
  "hook_event_name": "PermissionRequest",
  "tool_name": "Bash",
  "tool_input": {"command": "git status", "description": "check"},
  "agent_id": "agent-abc"
}
```

Emitter shape (`docs/claude-code-hook-schema.md:90`):
`{...$f(…), hook_event_name:"PermissionRequest", tool_name, tool_input, permission_suggestions}`.

### 1.6 PostToolUse — emitter shape and live subagent observation

Emitter (`docs/claude-code-hook-schema.md:93`):

```js
{...$f(…), hook_event_name:"PostToolUse", tool_name, tool_input, tool_response, tool_use_id, duration_ms}
```

**(reconstructed)** subagent variant, values taken from the live observation at
`docs/claude-code-hook-schema.md:222-226` (three teammates, one session):

```json
{
  "session_id": "191d2044-....",
  "transcript_path": "/home/tjmisko/.claude/projects/.../DigestDownloads/191d2044-….jsonl",
  "cwd": "/home/tjmisko/Projects/DigestDownloads",
  "permission_mode": "default",
  "agent_id": "ac24f297af282c150",
  "agent_type": "general-purpose",
  "hook_event_name": "PostToolUse",
  "tool_name": "Bash",
  "tool_input": {"command": "git status"},
  "tool_response": {"stdout": "...", "stderr": "", "interrupted": false},
  "tool_use_id": "toolu_01...",
  "duration_ms": 412
}
```

Note `agent_id` arrives **bare** (no `agent-` prefix) and `transcript_path`
stays the parent's (`docs/claude-code-hook-schema.md:213-216`).

### 1.7 Stop — test fixture

`cmd/switchboard-ctl/hookinput_test.go:385-389`:

```json
{
  "session_id": "sess-1",
  "transcript_path": "/tmp/t.jsonl",
  "hook_event_name": "Stop"
}
```

### 1.8 SubagentStart / SubagentStop

`docs/claude-code-hook-schema.md:158-162`: both carry `agent_id`, `agent_type`,
`session_id` (parent), `cwd`; Stop adds `agent_transcript_path`; neither carries
`tool_use_id`. **(reconstructed)**:

```json
{
  "session_id": "191d2044-....",
  "transcript_path": "/home/tjmisko/.claude/projects/.../191d2044-….jsonl",
  "cwd": "/home/tjmisko/Projects/DigestDownloads",
  "permission_mode": "default",
  "agent_id": "a1a4f5cc80cc6d9c2",
  "agent_type": "general-purpose",
  "hook_event_name": "SubagentStop",
  "agent_transcript_path": "/home/tjmisko/.claude/projects/.../191d2044-…/subagents/agent-a1a4f5cc80cc6d9c2.jsonl"
}
```

### 1.9 PermissionRequest vs PostToolUse field table

`docs/claude-code-hook-schema.md:102-107`:

| event | `tool_name` | `tool_input` | `tool_use_id` | other |
|---|---|---|---|---|
| PreToolUse | yes | yes | yes | — |
| PermissionRequest | yes | yes | **no** | `permission_suggestions` |
| PostToolUse | yes | yes | yes | `tool_response`, `duration_ms` |

PermissionRequest **lacks** `tool_use_id`, `tool_response`, `duration_ms`.
`tool_input` on PostToolUse is `updatedInput` (post-approval rewrite), so it is
not byte-stable versus PermissionRequest (`schema.md:129-154`).

### 1.10 Hook registration (Claude)

`README.md:351-359`:

```json
"hooks": {
  "SessionStart":      [{ "hooks": [{ "type": "command", "command": "switchboard-ctl hook SessionStart",      "timeout": 2 }] }],
  "UserPromptSubmit":  [{ "hooks": [{ "type": "command", "command": "switchboard-ctl hook UserPromptSubmit",  "timeout": 2 }] }],
  "PreToolUse":        [{ "matcher": "AskUserQuestion|ExitPlanMode", "hooks": [{ "type": "command", "command": "switchboard-ctl hook PreToolUse", "timeout": 2 }] }],
  "PostToolUse":       [{ "hooks": [{ "type": "command", "command": "switchboard-ctl hook PostToolUse",       "timeout": 2 }] }],
  "PermissionRequest": [{ "hooks": [{ "type": "command", "command": "switchboard-ctl hook PermissionRequest", "timeout": 2 }] }],
  "Stop":              [{ "hooks": [{ "type": "command", "command": "switchboard-ctl hook Stop",              "timeout": 2 }] }],
  "SubagentStart":     [{ "hooks": [{ "type": "command", "command": "switchboard-ctl hook SubagentStart",     "timeout": 2 }] }],
  "SubagentStop":      [{ "hooks": [{ "type": "command", "command": "switchboard-ctl hook SubagentStop",      "timeout": 2 }] }]
}
```

---

## 2. Codex hook stdin payloads

**Transport:** identical to §1 but via `switchboard-ctl codex-hook <Event>`
(`main.go:125-129`), configured in `~/.codex/hooks.json` (`README.md:396-404`,
same eight events). Codex emits `PreToolUse` on every tool. Extra fields read:
`source` (SessionStart: `startup|resume|clear|compact`), `turn_id`,
`tool_use_id`, `permission_mode`, and the two bounded naming fields `prompt`
(UserPromptSubmit only) and `last_assistant_message` (Stop only), each cut to
1000 runes (`main.go:684-689`, `docs/codex-session-naming-findings.md:25-28`).
Codex also gets `hook_client_hints` appended by the ctl from `SSH_TTY`,
`WEZTERM_PANE`, `TMUX_PANE`, and `/proc/self/fd` (`main.go:709-746`).

### 2.1 PreToolUse — test fixture

`cmd/switchboard-ctl/hookinput_test.go:243-251`:

```json
{
  "session_id": "thread-new",
  "hook_event_name": "PreToolUse",
  "source": "clear",
  "turn_id": "turn-7",
  "tool_use_id": "call-9",
  "permission_mode": "plan",
  "tool_name": "request_user_input"
}
```

### 2.2 UserPromptSubmit / Stop — naming fields (test fixture)

`cmd/switchboard-ctl/hookinput_test.go:264-265` (values shortened):

```json
{"session_id":"thread-1","prompt":"  界界界…(1002 runes)  ","last_assistant_message":"  🙂🙂…(1002 runes)  "}
```

Parsed as `UserPromptSubmit` → only `prompt` forwarded (1000 runes, trimmed);
parsed as `Stop` → only `last_assistant_message`; any other event or the
`claude` agent → neither (`hookinput_test.go:267-282`).

### 2.3 SubagentStart — test fixture

`cmd/switchboard-ctl/hookinput_test.go:286-292`:

```json
{
  "session_id": "root-thread",
  "hook_event_name": "SubagentStart",
  "agent_id": "child-thread",
  "agent_type": "worker",
  "turn_id": "turn-7"
}
```

For Codex, `agent_id` is the **child thread id** and `session_id` the root
thread id; the coordinator applies it only when `agent_id` names a non-root
node in a fresh app-server graph (`docs/codex-investigation.md:214-218`).

### 2.4 SessionStart / PermissionRequest / PostToolUse — (reconstructed)

Shape follows §2.1; `source` is meaningful only on SessionStart. The Go
tests drive these as `rpc.Request` rather than stdin, e.g.
`cmd/switchboard/codex_hook_transition_test.go:67-70`:

```go
rpc.Request{Event: "PermissionRequest", SessionID: "thread-1", TurnID: "turn-1",
            ToolUseID: "call-1", ToolName: "exec_command", ObservedAt: base}
```

```json
{"session_id":"thread-1","hook_event_name":"SessionStart","source":"startup","permission_mode":"default"}
{"session_id":"thread-1","hook_event_name":"PermissionRequest","turn_id":"turn-1","tool_use_id":"call-1","tool_name":"exec_command"}
{"session_id":"thread-1","hook_event_name":"PostToolUse","turn_id":"turn-1","tool_use_id":"call-1","tool_name":"exec_command"}
```

Event → status for Codex (`internal/rpc/rpc.go:1461-1470`):
`UserPromptSubmit|PreToolUse|PostToolUse → working`, `PermissionRequest →
permission`, `Stop|SessionStart → idle`. `AskUserQuestion` or
`request_user_input` on PreToolUse/PermissionRequest opens a user-input wait
keyed by `tool_use_id` (`docs/codex-investigation.md:187-196`).

---

## 3. `rpc.Request` / `rpc.Response` on the daemon socket

**Transport:** Unix socket, mode 0600, one JSON object per line each way
(`internal/rpc/rpc.go:1-9`, `322-330`, `373-380`). Producers: `switchboard-ctl`
(hooks, focus, pane-bind, activity), WezTerm Lua via ctl, bar renderers
(subscribe). Struct tags at `rpc.go:48-149`; response at `rpc.go:166-171`.
`subscribe` is terminal on its connection: the server streams
`{"snapshot":…}` frames until the client closes (`rpc.go:511-545`). The
daemon reads: `cmd`, `selector`, `hostname`/`pid`/`started_at`,
`binding`/`gui_pid`/`window_id`/`pane_id`/`window_focused`, every hook field,
`activity`.

### 3.1 `hook` — Claude PermissionRequest as sent by the ctl

Built by `parseHookPayload` + `cmdHook` (`main.go:629-691`) from §1.5.
`tool_input_hash` is a 16-hex sha256 prefix of the canonicalized input
(`main.go:752-770`); the raw input never crosses. **(reconstructed)** from the
struct tags:

```json
{
  "cmd": "hook",
  "event": "PermissionRequest",
  "pid": 1090904,
  "session_id": "sess-1",
  "observed_at": "2026-08-05T17:21:03.412Z",
  "transcript": "/tmp/t.jsonl",
  "agent": "claude",
  "tool_name": "Bash",
  "tool_input_hash": "3c81de52b9a0f74e",
  "agent_id": "agent-abc"
}
```

`pid` is `os.Getppid()` of the ctl (`main.go:632`); the daemon walks the ppid
chain to the nearest tracked agent (`rpc.go:1407-1422`). The daemon strips one
`agent-` prefix from `agent_id` (`rpc.go:1134-1140`).

### 3.2 `hook` — Codex PreToolUse as sent by the ctl

From §2.1 plus `hook_client_hints` (`hookinput_test.go:299-311`):

```json
{
  "cmd": "hook",
  "event": "PreToolUse",
  "pid": 412233,
  "session_id": "thread-new",
  "observed_at": "2026-08-24T10:02:11.008Z",
  "hook_source": "clear",
  "turn_id": "turn-7",
  "tool_use_id": "call-9",
  "permission_mode": "plan",
  "agent": "codex",
  "tool_name": "request_user_input",
  "hook_client_hints": [
    {"kind": "tty", "value": "/dev/pts/7"},
    {"kind": "wezterm_pane", "value": "12"},
    {"kind": "tmux_pane", "value": "%3"}
  ]
}
```

Response for every hook: `{"ok":true}` (`rpc.go:451-453`).

### 3.3 `list` / `list-all`

```json
{"cmd":"list"}
```
→ `{"snapshot":{ …state.Snapshot… }}` (`rpc.go:384-389`). The snapshot body is
exactly the `state.json` document in §11. `list-all` returns the federated
aggregate when configured (`rpc.go:473-478`).

### 3.4 `subscribe` / `subscribe-all`

`internal/rpc/subscribe_test.go:61`:

```json
{"cmd":"subscribe"}
```
→ immediate `{"snapshot":{…}}` then one frame per store broadcast, newline
delimited (`rpc.go:523-545`). Renderers use `subscribe-all`
(`internal/waybarchip/waybarchip.go:170`, `cmd/claude-tui/main.go:62`).

### 3.5 `focus` / `focus-session`

```json
{"cmd":"focus","selector":"idx:0"}
```
Selectors: `""|active`, `pid:<n>`, `idx:<n>`, bare `<n>` (`rpc.go:631-670`).

`cmd/switchboard-ctl/main.go:220` and `internal/rpc/federation_test.go:160`:

```json
{"cmd":"focus-session","hostname":"goosebook","pid":4821,"started_at":"2026-05-28T09:00:00Z"}
```
→ `{"ok":true}` or `{"error":"…"}` (`rpc.go:403-414`).

### 3.6 `activity`

`cmd/switchboard-ctl/main.go:383`, validated at `rpc_test.go:235-246`:

```json
{"cmd":"activity","activity":"idle"}
{"cmd":"activity","activity":"active"}
```
Any other value → `{"error":"…"}`.

### 3.7 `pane-bind` / `pane-state` / `announce-bindings`

`main.go:338`, `main.go:349-353`, `federation_test.go:161-162`:

```json
{"cmd":"pane-bind","binding":"{\"v\":1,\"host\":\"goosebook\",\"pid\":4821,\"started_at\":\"2026-05-28T09:00:00Z\"}","gui_pid":4790,"window_id":3,"pane_id":12}
{"cmd":"pane-state","gui_pid":4790,"window_id":3,"pane_id":12,"window_focused":true}
{"cmd":"announce-bindings"}
```
`window_focused` is `omitempty`, so `false` is absent on the wire.

### 3.8 `agent-diagnostics`

```json
{"cmd":"agent-diagnostics"}
```
→ `{"ok":true,"diagnostics":[{"provider":"codex","category":"observer_initialized","count":3,"last_at":"…"}]}`
(`rpc.go:193-198`, `454-459`).

---

## 4. Codex app-server JSON-RPC (stdio)

**Transport:** the daemon spawns `codex app-server --stdio`
(`internal/provider/codex/proxy.go:98`) and speaks JSONL over its stdin/stdout.
Requests carry `{"id","method","params"}` (`client.go:122-126`); notifications
have no `id`; server→client requests carry an `id` the daemon never answers
(read-only allowlist, `client.go:19-31`). Cadence: resnapshot every 10 s,
freshness 15 s (`docs/codex-investigation.md:120-123`). Every fixture below is
schema-derived 0.149.0 synthetic data (`testdata/captures/manifest.json:1-60`).
Fields read: `notificationParams` at `observer.go:833-854` and the `rpc*`
structs at `protocol.go:13-70`.

### 4.1 initialize (request/response)

`testdata/captures/initialize.jsonl:1-2`:

```json
{"id":1,"method":"initialize","params":{"clientInfo":{"name":"switchboard-evidence","title":"Switchboard evidence capture","version":"0.0.0"},"capabilities":{"experimentalApi":true}}}
{"id":1,"result":{"userAgent":"codex_app_server/0.149.0","platformFamily":"unix","platformOs":"linux","codexHome":"/synthetic-codex-home"}}
```

Production sends `clientInfo {name:"switchboard",title:"Switchboard",version:"1"}`
then the `initialized` notification (`observer.go:425-436`); only
`result.userAgent` is read (`protocol.go:13-15`).

### 4.2 thread/read

`testdata/captures/root-with-two-children.jsonl:1-2`:

```json
{"id":10,"method":"thread/read","params":{"threadId":"01890f00-0000-7000-8000-000000000001","includeTurns":false}}
{"id":10,"result":{"thread":{"id":"01890f00-0000-7000-8000-000000000001","sessionId":"01890f00-0000-7000-8000-000000000001","createdAt":1700000000,"updatedAt":1700000000,"status":{"type":"idle"},"source":"cli","threadSource":"user","modelProvider":"openai","cliVersion":"0.149.0","cwd":"/synthetic-workspace","preview":"","turns":[],"ephemeral":false,"projectId":null}}}
```

Production uses `includeTurns:true` (`observer.go:503`) and reads `thread.id`,
`parentThreadId`, `name`, `agentNickname`, `agentRole`, `createdAt`,
`updatedAt`, `status{type,activeFlags}`, `source`, `turns[]` (`protocol.go:32-43`).

### 4.3 thread/list (descendants)

`testdata/captures/thread-list-descendants.json:10-11` (ancestor query; one
row shown):

```json
{"id":21,"method":"thread/list","params":{"ancestorThreadId":"01890f00-0000-7000-8000-000000000001","archived":false,"limit":100,"sortKey":"created_at","sortDirection":"asc","useStateDbOnly":true}}
{"id":21,"result":{"data":[
  {"id":"01890f00-0000-7000-8000-000000000002","sessionId":"01890f00-0000-7000-8000-000000000001","parentThreadId":"01890f00-0000-7000-8000-000000000001","agentNickname":"Atlas","agentRole":"researcher","createdAt":1700000001,"updatedAt":1700000020,"status":{"type":"notLoaded"},"source":{"subAgent":{"thread_spawn":{"parent_thread_id":"01890f00-0000-7000-8000-000000000001","depth":1,"agent_path":"agents/researcher","agent_nickname":"Atlas","agent_role":"researcher"}}},"threadSource":"subagent","modelProvider":"openai","cliVersion":"0.149.0","cwd":"/synthetic-workspace","preview":"","turns":[],"ephemeral":false,"projectId":null}
],"nextCursor":null,"backwardsCursor":"synthetic-ancestor-cursor"}}
```

Production adds `"sourceKinds":["subAgent","subAgentReview","subAgentCompact","subAgentThreadSpawn",…]`
(`observer.go:56-60`, `524-528`) and pages on `nextCursor`.

### 4.4 thread/started (child spawn)

`root-with-two-children.jsonl:4`:

```json
{"method":"thread/started","params":{"thread":{"id":"01890f00-0000-7000-8000-000000000002","sessionId":"01890f00-0000-7000-8000-000000000001","parentThreadId":"01890f00-0000-7000-8000-000000000001","agentNickname":"Atlas","agentRole":"researcher","createdAt":1700000001,"updatedAt":1700000001,"status":{"type":"active","activeFlags":[]},"source":{"subAgent":{"thread_spawn":{"parent_thread_id":"01890f00-0000-7000-8000-000000000001","depth":1,"agent_path":"agents/researcher","agent_nickname":"Atlas","agent_role":"researcher"}}},"threadSource":"subagent","modelProvider":"openai","cliVersion":"0.149.0","cwd":"/synthetic-workspace","preview":"","turns":[],"ephemeral":false,"projectId":null}}}
```

A `source.subAgent.other == "guardian"` thread marks its parent's wait as
automatic (`wait_ownership_test.go:35-47`).

### 4.5 thread/status/changed with activeFlags

`child-waiting-approval.jsonl:1` and `child-waiting-user-input.jsonl:1`:

```json
{"method":"thread/status/changed","params":{"threadId":"01890f00-0000-7000-8000-000000000002","status":{"type":"active","activeFlags":["waitingOnApproval"]}}}
{"method":"thread/status/changed","params":{"threadId":"01890f00-0000-7000-8000-000000000003","status":{"type":"active","activeFlags":["waitingOnUserInput"]}}}
{"method":"thread/status/changed","params":{"threadId":"01890f00-0000-7000-8000-000000000001","status":{"type":"idle"}}}
```

`type ∈ notLoaded|idle|active|systemError`; flags other than the two above set
`unknownEnum` (`protocol.go:238-265`).

### 4.6 turn/started and turn/completed

`root-with-two-children.jsonl:11`, `drain-to-idle.jsonl:1`:

```json
{"method":"turn/started","params":{"threadId":"01890f00-0000-7000-8000-000000000002","turn":{"id":"01890f00-0000-7000-8000-000000000011","status":"inProgress","items":[],"startedAt":1700000002}}}
{"method":"turn/completed","params":{"threadId":"01890f00-0000-7000-8000-000000000002","turn":{"id":"01890f00-0000-7000-8000-000000000011","status":"completed","items":[],"startedAt":1700000002,"completedAt":1700000020,"durationMs":18000}}}
```

`turn/started` on the root begins a new cohort (`observer.go:911-918`);
`turn/completed` clears that thread's wait state (`observer.go:919-923`).

### 4.7 item/started with collabAgentToolCall and agentsStates

`root-with-two-children.jsonl:3` (spawnAgent) and `drain-to-idle.jsonl:8`
(closeAgent completed):

```json
{"method":"item/started","params":{"threadId":"01890f00-0000-7000-8000-000000000001","turnId":"01890f00-0000-7000-8000-000000000010","startedAtMs":1700000001000,"item":{"id":"01890f00-0000-7000-8000-000000000020","type":"collabAgentToolCall","tool":"spawnAgent","status":"inProgress","senderThreadId":"01890f00-0000-7000-8000-000000000001","receiverThreadIds":["01890f00-0000-7000-8000-000000000002"],"prompt":null,"model":null,"reasoningEffort":null,"agentsStates":{"01890f00-0000-7000-8000-000000000002":{"status":"pendingInit"}}}}}
{"method":"item/completed","params":{"threadId":"01890f00-0000-7000-8000-000000000001","turnId":"01890f00-0000-7000-8000-000000000010","completedAtMs":1700000025000,"item":{"id":"01890f00-0000-7000-8000-000000000025","type":"collabAgentToolCall","tool":"closeAgent","status":"completed","senderThreadId":"01890f00-0000-7000-8000-000000000001","receiverThreadIds":["01890f00-0000-7000-8000-000000000002","01890f00-0000-7000-8000-000000000003"],"prompt":null,"model":null,"reasoningEffort":null,"agentsStates":{"01890f00-0000-7000-8000-000000000002":{"status":"shutdown"},"01890f00-0000-7000-8000-000000000003":{"status":"shutdown"}}}}}
```

`tool ∈ spawnAgent|sendInput|resumeAgent|wait|closeAgent`
(`observer.go:643-651`); `agentsStates[*].status ∈
pendingInit|running|completed|interrupted|errored|shutdown|notFound`
(`protocol.go:916-928`). Only `item.type == "collabAgentToolCall"` is applied
(`protocol.go:267-293`).

### 4.8 item/commandExecution/requestApproval (server request) + resolution

`child-waiting-approval.jsonl:2-3`:

```json
{"id":"approval-1","method":"item/commandExecution/requestApproval","params":{"threadId":"01890f00-0000-7000-8000-000000000002","turnId":"01890f00-0000-7000-8000-000000000011","itemId":"01890f00-0000-7000-8000-000000000030","startedAtMs":1700000010000,"command":null,"reason":null,"approvalId":null}}
{"id":"approval-1","result":{"decision":"accept"}}
```

Sibling methods handled identically: `item/fileChange/requestApproval`,
`item/permissions/requestApproval`; legacy `applyPatchApproval` /
`execCommandApproval` key on `conversationId`+`callId` (`observer.go:967-990`).
The request `id` may be a string or integer (`wait_ownership_test.go:56-57`).

### 4.9 item/tool/requestUserInput (server request) + resolution

`child-waiting-user-input.jsonl:2-3`:

```json
{"id":"user-input-1","method":"item/tool/requestUserInput","params":{"threadId":"01890f00-0000-7000-8000-000000000003","turnId":"01890f00-0000-7000-8000-000000000012","itemId":"01890f00-0000-7000-8000-000000000031","isBlocking":true,"autoResolutionMs":null,"questions":[{"id":"choice","header":"Synthetic","question":"[sanitized]","isOther":false,"isSecret":false,"options":[{"label":"A","description":"[sanitized]"},{"label":"B","description":"[sanitized]"}]}]}}
{"id":"user-input-1","result":{"answers":{"choice":{"answers":["[sanitized]"]}}}}
```

Attention `user_input` only when `isBlocking && autoResolutionMs == null`
(`observer.go:991-1009`, `wait_ownership_test.go:324-355`).

### 4.10 serverRequest/resolved

`wait_ownership_test.go:498-503`:

```json
{"method":"serverRequest/resolved","params":{"threadId":"root","requestId":"approval-1"}}
{"method":"serverRequest/resolved","params":{"threadId":"root","requestId":1}}
```

### 4.11 thread/settings/updated

`wait_ownership_test.go:52-54`:

```json
{"method":"thread/settings/updated","params":{"threadId":"root","threadSettings":{"approvalsReviewer":"user"}}}
```

`approvalsReviewer ∈ user|auto_review|guardian_subagent`
(`wait_ownership_test.go:21-25`, `protocol.go:455-459`).

### 4.12 item/autoApprovalReview/started and /completed

`wait_ownership_test.go:505-511`:

```json
{"method":"item/autoApprovalReview/started","params":{"threadId":"root","turnId":"turn","reviewId":"review","targetItemId":"item","review":{"status":"allow"}}}
{"method":"item/autoApprovalReview/completed","params":{"threadId":"root","turnId":"turn","reviewId":"review","targetItemId":"item","review":{"status":"deny"}}}
```

Only `threadId`, `reviewId`, `targetItemId` are read (`observer.go:960-966`).

### 4.13 Other notifications consumed

`thread/updated` (same params as started), `thread/name/updated`
(`threadId`, `threadName`), `thread/archived`, `thread/deleted` (`threadId`),
`mcpServer/elicitation/request` (server request; `threadId`,`turnId`,`itemId`)
(`observer.go:876-1021`).

---

## 5. Codex rollout files

**Transport:** files under `~/.codex/sessions`, written by Codex itself.
`docs/codex-investigation.md:268-280`:

> The observed layout is `~/.codex/sessions/YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl`,
> with records shaped as `timestamp`, `type`, and `payload`.

Example path from the golden fixture (`internal/state/testdata/state.golden.json:87`):

```
/home/tjmisko/.codex/sessions/2026/05/28/rollout-2026-05-28T09-02-00-0199736b-b713-74e2-99a2-f015a1c42816.jsonl
```

**(reconstructed)** line shape, from the three-field description above; no
literal rollout lines exist in the repo:

```json
{"timestamp":"2026-05-28T09:02:00.000Z","type":"session_meta","payload":{"id":"0199736b-b713-74e2-99a2-f015a1c42816","cwd":"/home/tjmisko/Projects/api"}}
{"timestamp":"2026-05-28T09:02:10.000Z","type":"turn_context","payload":{"turn_id":"…"}}
```

**Daemon use:** none at runtime. `rg rollout internal/provider/codex` hits only
`binding.go:12` ("rollout recency [is] deliberately not [a] binding source") and
the `-codex-observer` rollout-gate comment (`cmd/switchboard/main.go:66`). The
rollout path is stored as `codex.transcript` (from the hook's
`transcript_path`) and is otherwise unread; the investigation rejected rollout
tailing because it cannot distinguish executing from blocked-on-approval
(`docs/codex-investigation.md:275-280`).

---

## 6. Hyprland IPC

**Transport:** two Unix sockets under `$XDG_RUNTIME_DIR/hypr/<sig>/`:
`.socket.sock` (request/response, text, one connection per command) and
`.socket2.sock` (event stream, one line per event) (`internal/hyprland/hyprland.go:241-254`).
Producer: the compositor. Cadence: socket2 fires on every window/focus/title
change; requests happen on reconcile (`j/clients`, `j/activewindow`) and on
focus. The parser splits each line on the **first** `>>` into `Event{Name,Data}`
(`hyprland.go:163-168`, `199-214`); the wm seam keeps five names and, for
non-layout events, treats `Data` as the bare address and prefixes `0x`
(`internal/wm/hyprland.go:86-105`).

### 6.1 socket2 lines — repo fixtures

`internal/hyprland/hyprland_test.go:182`, `:237`; `internal/testsupport/harness_test.go:14`:

```
activewindowv2>>0x5,foo>>bar
openwindow>>0x6
activewindowv2>>0x5
activewindowv2>>2a3b
closewindow>>2a3b
```

### 6.2 socket2 lines — live formats (Hyprland wiki, not repo fixtures)

The five names the daemon handles (`wm/hyprland.go:88-94`, `:115`):

```
activewindowv2>>5640f1a2b3c0
closewindow>>5640f1a2b3c0
openwindow>>5640f1a2b3c0,4,org.wezfurlong.wezterm,claude — switchboard
movewindowv2>>5640f1a2b3c0,4,4
windowtitlev2>>5640f1a2b3c0,claude — switchboard [sbw:4790:3]
```

Mapping: `closewindow→window-closed`, `activewindowv2→focus-changed`,
`movewindowv2|windowtitlev2|openwindow→layout-changed` (`wm/hyprland.go:88-94`).
Address-bearing events use the whole `Data` string as the address, so only
`activewindowv2` and `closewindow` (single-field payloads) yield a usable
address; layout events drop `Data` (`wm/hyprland.go:72-75`). The daemon reacts
in `handleWMEvent` (`cmd/switchboard/main.go:741-797`); layout events are
debounced before reaching it.

### 6.3 Request socket commands

```
j/clients
j/activewindow
j/getoption cursor:no_warps
/dispatch hl.dsp.focus({window="address:0xdeadbeef"})
```
(`hyprland.go:45`, `:60`, `:151`, `:81`, `:127-129`). `j/clients` rows are read
as `{address,pid,class,title,workspace{id,name},monitor,mapped}`
(`hyprland.go:27-40`).

### 6.4 Exact Lua focus batch

`internal/hyprland/hyprland.go:141-146`, pinned by `hyprland_test.go:277-287`:

```
[[BATCH]]eval hl.config({cursor={no_warps=true}}) ; dispatch hl.dsp.focus({window="address:0xabc"}) ; eval hl.config({cursor={no_warps=false}})
```

The trailing `no_warps=<prev>` restores the value read via
`j/getoption cursor:no_warps` (`{"int":0}`). Response is `ok` once per
sub-command, newline separated (`hyprland.go:116-121`). No semicolon may appear
inside a Lua chunk because ` ; ` is the batch separator.

---

## 7. i3 / sway IPC

**Transport:** Unix socket from `$SWAYSOCK` or `$I3SOCK`
(`internal/wm/i3.go:33-41`). Binary framing, native byte order: 6-byte magic
`i3-ipc`, u32 payload length, u32 type; event replies set bit 31
(`i3.go:179-197`, `224-256`). Producer: the compositor. Cadence: `GET_TREE`
on reconcile; events streamed on a dedicated connection after `SUBSCRIBE`.
Fields read: `id`, `type`, `name`, `focused`, `window`, `app_id`, `pid`,
`num`, `nodes`, `floating_nodes`, `window_properties.title` (`i3.go:260-274`);
event `change` and `container.id` (`i3.go:435-461`).

### 7.1 Frame layout

```
offset 0   "i3-ipc"        (6 bytes, ASCII magic)
offset 6   uint32 length   (native endian, payload bytes)
offset 10  uint32 type     (native endian; RUN_COMMAND=0, SUBSCRIBE=2, GET_TREE=4;
                            on replies bit 31 set => event, low bits: workspace=0, window=3)
offset 14  payload         (length bytes of JSON)
```

Round-trip pinned at `internal/wm/i3_test.go:10-26`; bad magic rejected
(`:28-34`).

### 7.2 SUBSCRIBE request and ack

`i3.go:111-116`:

```json
["window","workspace"]
```
→ first reply payload `{"success":true}`, then events on the same connection.

### 7.3 GET_TREE node example (trimmed)

`internal/wm/i3_test.go:39-52`:

```json
{
  "id": 1, "type": "root", "name": "root", "nodes": [
    {"id": 2, "type": "output", "name": "eDP-1", "nodes": [
      {"id": 3, "type": "workspace", "name": "1", "num": 1, "nodes": [
        {"id": 100, "type": "con", "name": "claude — proj", "app_id": "org.wezfurlong.wezterm",
         "pid": 4790, "focused": true, "nodes": [], "floating_nodes": []}
      ], "floating_nodes": []},
      {"id": 4, "type": "workspace", "name": "2", "num": 2, "nodes": [
        {"id": 200, "type": "con", "name": "vim", "window": 12582913,
         "window_properties": {"title": "vim"}, "focused": false, "nodes": [], "floating_nodes": []}
      ], "floating_nodes": []}
    ], "floating_nodes": []}
  ], "floating_nodes": []
}
```

Window ref is the decimal `id` (con_id); focus is `[con_id=100] focus`
(`i3.go:81`). i3 omits `pid`, so it is hydrated via X11 `_NET_WM_PID` on the
`window` id (`i3.go:351-388`).

### 7.4 Event payloads

`internal/wm/i3_test.go:141-146`:

```json
{"change":"close","container":{"id":100}}
{"change":"focus","container":{"id":200}}
{"change":"title","container":{"id":200}}
{"change":"move","container":{"id":200}}
{"change":"urgent","container":{"id":200}}
{"change":"focus"}
```

`close→window-closed`, `focus→focus-changed`, `new|move|title→layout-changed`,
`urgent` ignored; any workspace event → `layout-changed` (`i3.go:448-459`).

---

## 8. WezTerm and tmux enumeration

**Transport:** subprocess stdout. WezTerm: `wezterm cli list --format json`
run once per live mux with `WEZTERM_UNIX_SOCKET=$XDG_RUNTIME_DIR/wezterm/gui-sock-<pid>`
(`internal/wezterm/wezterm.go:52-81`, `145-175`), 2 s timeout. tmux:
`tmux list-panes -a -F <fmt>` (`internal/terminal/tmux.go:54`). Cadence: on
reconcile and on Locate; measured target ≤ 1 fork/s
(`docs/memory-footprint-plan.md:852`). Fields read: WezTerm `window_id`,
`tab_id`, `pane_id`, `title`, `window_title`, `cwd`, `tty_name`, `is_active`
(`wezterm.go:37-46`); tmux four tab-separated fields (`tmux.go:131-153`).

### 8.1 `wezterm cli list --format json` row

**(reconstructed)** from `cliPane` struct tags (`wezterm.go:37-46`); the repo
has no literal JSON fixture (tests construct `wezterm.Pane` values, e.g.
`internal/federation/navigator_test.go:61`):

```json
[
  {
    "window_id": 3,
    "tab_id": 7,
    "pane_id": 12,
    "workspace": "default",
    "size": {"rows": 48, "cols": 190, "pixel_width": 2280, "pixel_height": 1440, "dpi": 144},
    "title": "claude",
    "cwd": "file://goosebook/home/tjmisko/Projects/switchboard",
    "cursor_x": 0, "cursor_y": 0, "cursor_shape": "Default", "cursor_visibility": "Visible",
    "left_col": 0, "top_row": 0,
    "tty_name": "/dev/pts/3",
    "is_active": true,
    "is_zoomed": false
  }
]
```

Daemon output row adds `mux_pid` and `mux_socket`
(`wezterm.go:24-35`); the join key is `tty_name == session.tty`
(`wezterm.go:180-191`). The `window_title` key is read when present
(`wezterm.go:42`) though not all WezTerm builds emit it.

### 8.2 tmux list-panes format string and output

`internal/terminal/tmux.go:131`:

```
#{pane_tty}\t#{pane_id}\t#{pane_current_path}\t#{window_name}
```

Example stdout line **(reconstructed)** per `parseTmuxPanes` (`tmux.go:142-153`):

```
/dev/pts/9	%3	/home/tjmisko/Projects/api	codex
```

`no server running` on stderr is a normal empty result (`tmux.go:164-167`).
Activate is `select-window -t %3` then `select-pane -t %3` (`tmux.go:121-125`).

---

## 9. OSC 1337 SetUserVar and the window-title marker

**Transport:** the daemon opens the session's PTY (`O_NONBLOCK|O_NOFOLLOW`,
char-device checked) and writes one escape sequence
(`internal/panebind/osc.go:33-105`). WezTerm base64-decodes the value and fires
`user-var-changed` in Lua (`integrations/wezterm/switchboard.lua:246-257`),
which runs `switchboard-ctl pane-bind …` (§3.7). Cadence: on binding
announcement and remote-stream re-announce. The marker is appended by the Lua
`format-window-title` composer and read back by the daemon from the WM title
to find the OS window (`panebind/types.go:117-119`, `switchboard.lua:116-134`).

### 9.1 SetUserVar payload (decoded value)

`internal/panebind/payload.go:12-29`; `VariableName = "SWITCHBOARD_SESSION"`,
`MaxPayloadBytes = 512`, version 1, unknown fields rejected:

```json
{"v":1,"host":"goosebook","pid":4821,"started_at":"2026-05-28T09:00:00Z"}
```

`started_at` is RFC3339Nano UTC (`payload.go:33`, `types.go:56-59`).

### 9.2 Wire bytes

`osc.go:10`, `:43-65` — a clear assignment followed by the set assignment:

```
ESC ] 1337 ; SetUserVar=SWITCHBOARD_SESSION= BEL
ESC ] 1337 ; SetUserVar=SWITCHBOARD_SESSION=<base64(payload)> BEL
```

i.e. literally
`\x1b]1337;SetUserVar=SWITCHBOARD_SESSION=\a\x1b]1337;SetUserVar=SWITCHBOARD_SESSION=eyJ2IjoxLCJob3N0IjoiZ29vc2Vib29rIiwicGlkIjo0ODIxLCJzdGFydGVkX2F0IjoiMjAyNi0wNS0yOFQwOTowMDowMFoifQ==\a`.
Lua ignores the empty clear (`switchboard.lua:249`).

### 9.3 `[sbw:gui:window]` title marker

`internal/panebind/types.go:117-119` and `switchboard.lua:116-118`:

```
[sbw:%d:%d]        -> "[sbw:4790:3]"      (GUI pid, mux window id)
```

Composed title: `"<base title> [sbw:4790:3]"`, or just the marker when the base
title is empty; a stale trailing marker is stripped first
(`switchboard.lua:124-134`). The Lua callbacks then run
`switchboard-ctl pane-bind <payload> <gui_pid> <window_id> <pane_id>`
(`switchboard.lua:195-202`) and
`switchboard-ctl pane-state <gui_pid> <window_id> <pane_id> true|false`
(`switchboard.lua:287-294`).

---

## 10. Waybar slot output and bottom-bar handshake files

**Transport:** `switchboard-ctl bottombar watch` holds one `subscribe-all`
stream, renders all ten slots, and atomically replaces
`$XDG_RUNTIME_DIR/switchboard/slot-N.json` (`cmd/switchboard-ctl/bottombar.go:107`,
`:186`), then sends `SIGRTMIN+N` to the exact Waybar pidfd
(`bottombar.go:52`, `docs/bars/README.md:36-49`). Waybar reads one line per
signal (`waybar/claude.jsonc:23-32`). Producers: the ctl (slots, pid, visible)
and the Waybar module itself (ready). Cadence: on every snapshot change plus a
coarse-clock refresh timer.

### 10.1 Slot JSON (`waybarOutput`)

Struct at `internal/waybarchip/waybarchip.go:41-46`:
`{"text","tooltip"(omitempty),"class":[…],"alt"(omitempty)}`. Rendered slot
(`waybarchip.go:355-360`) **(reconstructed)** with class vocabulary from
`internal/barview/barview_test.go:19-34`:

```json
{"text":"sb·switchboard","tooltip":"<span size='small' …>switchboard</span> ~/Projects/switchboard\ngoosebook   ● permission · 3m\n…","class":["permission","focused"],"alt":"permission"}
```

Literal fixtures from the renderer:

```json
{"text":"","class":["empty"]}
{"text":"✕","tooltip":"switchboard not running","class":["tracker-down"]}
{"text":"","tooltip":"no agent sessions","class":["empty"]}
{"text":"*proj-a  proj-b","class":["multi"],"alt":"2"}
```

(`waybarchip.go:80`, `:139`, `:394`, `:405-409`.) Class lists seen in tests:
`["working","delegating","focused"]`, `["unknown","suspended","remote"]`,
`["unknown","remote","unnavigable"]`; `alt` is the colour class
(`permission|idle|working|unknown`, delegating → working).

### 10.2 `bottom-waybar.ready` (written by the Waybar module)

`waybar/claude.jsonc:23` writes `$PPID` then field 22 of `/proc/$PPID/stat`:

```
1234567 98765432
```

Legacy split mode accepts a PID-only line; combined mode requires both fields
and matches them against the live process (`bottombar.go:570-588`,
`:725-762`).

### 10.3 `bottom-waybar.pid` (written by the ctl in legacy mode)

`bottombar.go:694`:

```
1234567 98765432
```

### 10.4 `bottom-waybar.visible` (written by the ctl in combined mode)

`bottombar.go:822`, parsed at `:770-792`:

```
1234567 98765432 true
```

Paths: `$XDG_RUNTIME_DIR/switchboard/bottom-waybar.{pid,ready,visible}`
(`bottombar.go:218-221`). The master toggle is a marker file whose absence
means "top bar visible" (`bottombar.go:592-595`).

---

## 11. `state.json` golden fixture (schema v3)

**Transport:** file, `~/.cache/switchboard/state.json`, atomically rewritten by
the daemon on every change (`docs/bars/README.md:3-7`); the same document is
the `snapshot` body of `list`/`subscribe`. Full contents of
`internal/state/testdata/state.golden.json:1-153`:

```json
{
  "schema_version": 3,
  "sessions": [
    {
      "pid": 4821,
      "cwd": "/home/tjmisko/Projects/switchboard",
      "tty": "/dev/pts/3",
      "started_at": "2026-05-28T09:00:00Z",
      "focused": true,
      "suspended": true,
      "headless": true,
      "agent": "claude",
      "resolved_name": "manual-session-name",
      "wezterm": {
        "mux_pid": 4790,
        "mux_socket": "/run/user/1000/wezterm/gui-sock-4790",
        "pane_id": 12,
        "tab_id": 7,
        "window_id": 3,
        "window_title": "claude — switchboard"
      },
      "hyprland": {
        "address": "0x5640f1a2b3c0",
        "workspace": "4",
        "workspace_id": 4,
        "monitor": "DP-1"
      },
      "claude": {
        "session_id": "e0b4b21f-aaf6-4ab0-a8d6-2d595aba4065",
        "transcript": "/home/tjmisko/.claude/projects/switchboard/e0b4b21f.jsonl",
        "status": "working",
        "status_since": "2026-05-28T09:01:00Z",
        "in_flight_subagents": 2,
        "workflows": [
          {
            "run_id": "wf_5e3cb808-2ac",
            "name": "simplification-audit",
            "agents_started": 17,
            "agents_done": 7,
            "in_flight": 2
          }
        ],
        "pending_writers": [
          "af5bd126402ac16c7",
          "main"
        ],
        "pending_prompts": [
          {
            "writer": "af5bd126402ac16c7",
            "tool": "AskUserQuestion",
            "input_hash": "9f2c4a1b7d05e386",
            "attention": "user_input",
            "since": "2026-05-28T09:02:00Z"
          },
          {
            "writer": "main",
            "tool": "Bash",
            "input_hash": "3c81de52b9a0f74e",
            "attention": "approval",
            "since": "2026-05-28T09:02:30Z"
          },
          {
            "writer": "main",
            "tool": "Edit",
            "input_hash": "5a6072ce18bd493f",
            "attention": "approval",
            "since": "2026-05-28T09:02:45Z"
          }
        ]
      }
    },
    {
      "pid": 4999,
      "cwd": "/home/tjmisko/Projects/api",
      "tty": "/dev/pts/7",
      "started_at": "2026-05-28T09:02:00Z",
      "focused": false,
      "agent": "codex",
      "display_name": {
        "value": "api-cleanup",
        "origin": "generated",
        "conversation_id": "0199736b-b713-74e2-99a2-f015a1c42816",
        "native_baseline": "API maintenance"
      },
      "codex": {
        "session_id": "0199736b-b713-74e2-99a2-f015a1c42816",
        "transcript": "/home/tjmisko/.codex/sessions/2026/05/28/rollout-2026-05-28T09-02-00-0199736b-b713-74e2-99a2-f015a1c42816.jsonl",
        "status": "idle"
      },
      "agent_graph": {
        "root_id": "0199736b-b713-74e2-99a2-f015a1c42816",
        "source": "codex_app_server",
        "observed_at": "2026-05-28T09:04:00Z",
        "fresh_until": "2026-05-28T09:04:30Z",
        "complete": true,
        "summary": {
          "runtime": "idle",
          "attention": "none",
          "status": "idle",
          "live_children": 0,
          "waiting_nodes": 0,
          "approval_nodes": 0,
          "user_input_nodes": 0,
          "error_nodes": 0,
          "since": "2026-05-28T09:04:00Z"
        },
        "nodes": [
          {
            "id": "0199736b-b713-74e2-99a2-f015a1c42816",
            "nickname": "API maintenance",
            "role": "root",
            "description": "Coordinate the API maintenance session",
            "runtime": "idle",
            "attention": "none",
            "lifecycle": "running",
            "started_at": "2026-05-28T09:02:00Z",
            "updated_at": "2026-05-28T09:04:00Z"
          },
          {
            "id": "child-review",
            "parent_id": "0199736b-b713-74e2-99a2-f015a1c42816",
            "nickname": "reviewer",
            "role": "explorer",
            "description": "Review the API surface",
            "runtime": "idle",
            "attention": "none",
            "lifecycle": "completed",
            "started_at": "2026-05-28T09:02:30Z",
            "updated_at": "2026-05-28T09:03:30Z",
            "completed_at": "2026-05-28T09:03:30Z",
            "usage": {
              "input_tokens": 1200,
              "cached_input_tokens": 800,
              "cache_write_input_tokens": 50,
              "output_tokens": 300,
              "reasoning_output_tokens": 75,
              "total_tokens": 2425,
              "model_context_window": 200000
            }
          }
        ]
      }
    },
    {
      "pid": 5102,
      "cwd": "/home/tjmisko/Tools/other",
      "tty": "/dev/pts/9",
      "started_at": "2026-05-28T09:05:00Z",
      "focused": false
    }
  ],
  "updated_at": "2026-05-28T09:05:30Z"
}
```

---

## 12. Hydrate-only persisted fields

**Transport:** the same `state.json`; read once at daemon start by `Load`
(`internal/state/state.go:1369`, `1419-1425`) to rebuild in-memory state
that no hook will ever re-raise. Producers: the daemon's own `enrichForWire`
projection. Cadence: every snapshot write; consumed only at hydrate.

### 12.1 `pending_writers` — key set of the in-memory `Pending` map

`state.go:281`, contract at `docs/state-schema.md:173`. Sorted ascending; bare
subagent id, or the literal `"main"` (`state.go:348`):

```json
"pending_writers": ["af5bd126402ac16c7", "main"]
```

Hydrate: a `permission` chip with no persisted writers seeds `["main"]`
(`cmd/switchboard/pending_hydrate_test.go:241-257`, fixture at `:243-244`):

```json
{"sessions":[{"pid":42,"cwd":"/p","tty":"/dev/pts/1","started_at":"2026-05-28T09:00:00Z","agent":"claude","claude":{"session_id":"5318eb5b-79df-4dee-a9f8-c80df4eca79e","status":"permission"}}],"updated_at":"2026-05-28T09:05:00Z"}
```

### 12.2 `pending_prompts` — one record per open call

`state.go:310`, `393-400`; contract `docs/state-schema.md:174`. Fields
`writer` (required), `tool`, `input_hash`, `call_id`, `attention`
(`approval|user_input`), `since` (RFC3339). Grouped by writer in
`pending_writers` order, oldest first:

```json
"pending_prompts": [
  {"writer": "af5bd126402ac16c7", "tool": "AskUserQuestion", "input_hash": "9f2c4a1b7d05e386", "attention": "user_input", "since": "2026-05-28T09:02:00Z"},
  {"writer": "main", "tool": "Bash", "input_hash": "3c81de52b9a0f74e", "call_id": "toolu_01ABC", "attention": "approval", "since": "2026-05-28T09:02:30Z"}
]
```

Every `writer` here also appears in `pending_writers`; the reverse need not
hold (old mirror). Hydrated prompts drop `Tool`/`InputHash` from the in-memory
map so the hook fast path cannot fire on them (`pending_hydrate_test.go:226-231`).

### 12.3 `display_name` — Codex display label

`internal/state/display_name.go:24-29`, `state.go:72`;
`docs/codex-session-naming-findings.md:70-79`:

```json
"display_name": {
  "value": "context-aware-session-names",
  "origin": "generated",
  "conversation_id": "<codex-thread-id>",
  "native_baseline": "session-naming"
}
```

`origin ∈ generated|fallback`; `native_baseline` omitted until an
authoritative native observation exists, and an explicit `""` means an
authoritative empty native name. Valid only while `conversation_id` matches the
live Codex thread (`display_name.go:34-42`).
