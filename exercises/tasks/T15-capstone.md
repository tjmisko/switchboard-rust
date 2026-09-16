# T15 — Capstone: run it on your own agents

**Core: 30m · Stretch: open-ended**

Steps 1 to 4 prove the daemon runs. Step 5 is the capstone: it is the first
time your T03 adapter, T11 core and T12 socket meet a real harness. If you
cannot run step 5 on this machine, step 6 replays a captured hook sequence
into the socket instead and shows the same transitions.

## Core: see it work

1. **Delete `#![allow(unused)]` from `lib.rs`** and get
   `cargo clippy --all-targets` clean. If every task is done it hides
   nothing: what it hid along the way was the imports each stub carried for
   the body you had not written yet, and the golden is clean without the
   line. A warning that does appear now is an import or helper you never
   used, which means you built something differently from the golden; read
   what it was for before deleting it. `model.rs` keeps its own
   `#![allow(dead_code)]`. It covers section 10 and the types its header
   lists as touched by no task, and it stays.

2. `cargo build --release`. Two binaries: `target/release/switchboard`, the
   daemon, and `target/release/switchboard-hook`, the forwarder.

3. **Run the daemon** against the real machine, on its own socket and its own
   output paths, so it collides with nothing (the Go daemon, or another copy
   of this course on the same machine). The three places it writes:

   | What | Where | Env |
   |---|---|---|
   | socket | `$XDG_RUNTIME_DIR/switchboard-rust.sock` | `SWITCHBOARD_RUST_SOCKET` |
   | `state.json` | `~/.cache/switchboard-rust/state.json` | `XDG_CACHE_HOME` |
   | history | `~/.local/state/switchboard-rust/history/<YYYY-MM-DD>.jsonl` | `XDG_STATE_HOME` |

   ```sh
   export SWITCHBOARD_RUST_SOCKET=$XDG_RUNTIME_DIR/sbr.sock
   XDG_CACHE_HOME=~/.cache/sbr XDG_STATE_HOME=~/.cache/sbr timeout 60 ./target/release/switchboard
   ```

   Within a second it should discover every running `claude` and replay each
   transcript tail. `bind` refuses a live socket: a second copy on the same
   name fails with `a daemon is already listening on …` rather than stealing
   it, and a socket left behind by a crashed daemon is removed. A socket path
   must be shorter than 108 bytes (`SUN_LEN`), which is why it lives under
   `$XDG_RUNTIME_DIR` and not under a project directory. `timeout` exits 124
   when it fires, even though the daemon exits cleanly on SIGTERM and removes
   its socket; that is `timeout` reporting on itself, not a failure.

4. **In another shell, list and subscribe.** `socat` is not installed on
   every machine, so here is the client, verbatim. Save it as `sbr-watch.py`:

   ```python
   #!/usr/bin/env python3
   """Subscribe to the daemon and print every agent on every snapshot.

   usage: python3 sbr-watch.py [SOCKET]   (default: $SWITCHBOARD_RUST_SOCKET)
   """
   import json
   import os
   import socket
   import sys

   path = sys.argv[1] if len(sys.argv) > 1 else os.environ["SWITCHBOARD_RUST_SOCKET"]
   sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
   sock.connect(path)
   sock.sendall(b'{"cmd":"subscribe"}\n')
   for line in sock.makefile("r"):
       snap = json.loads(line)["snapshot"]
       print(f"-- updated_ms={snap['updated_ms']}")
       for a in snap["agents"]:
           print(f"{a['pid']:>7}  {a['name']:<28} {a['phase']:<9} {a['lamp']:<5} "
                 f"truth={a['truth']:<9} {a['reason']}")
       sys.stdout.flush()
   ```

   It prints one block per snapshot: one now, then one per change, so a
   quiet machine prints once and then nothing (R6). A one-shot list is the
   same with `{"cmd":"list"}` and one `readline()`. The snapshot is your own
   `View::to_json`: `schema`, `updated_ms`, and per agent `pid`, `start`,
   `session_id`, `name`, `cwd`, `phase`, `lamp`, `since_ms`, `truth`,
   `reason`, `focused`. Nothing on the machine renders this yet; there is no
   chip to watch. What you watch is `phase` and `lamp`, and what you check
   is `truth` against what you know each agent is doing. An agent that is
   mid-turn but waiting on something outside its transcript (a teammate, a
   long background job) shows `idle` after replay, because its last
   transcript lines are assistant text with no open call. That is the gap
   the hooks close; note which agents show it before step 5 and after.

   Also confirm: `state.json` exists and no `state.json.tmp` is left behind;
   the history file has one `session_start` row per agent and `shown` rows
   `unknown -> working -> idle` from the replay; a bad line on the socket
   (`printf 'nope\n'`) gets `{"ok":false,"error":"malformed JSON: …"}` and
   the connection stays up; the socket is mode 0600.

5. **Point one Claude session's hooks at the forwarder**, without touching
   your global settings. Save this as `~/.cache/sbr-hooks.json`, with
   `/ABS` replaced by the absolute path of your checkout (the hook binary is
   not on `PATH`, and Claude runs hook commands through `sh`):

   ```json
   {
     "hooks": {
       "UserPromptSubmit":   [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook UserPromptSubmit",   "timeout": 2 }] }],
       "PreToolUse":         [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook PreToolUse",         "timeout": 2 }] }],
       "PermissionRequest":  [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook PermissionRequest",  "timeout": 2 }] }],
       "PostToolUse":        [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook PostToolUse",        "timeout": 2 }] }],
       "PostToolUseFailure": [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook PostToolUseFailure", "timeout": 2 }] }],
       "Stop":               [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook Stop",               "timeout": 2 }] }],
       "StopFailure":        [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook StopFailure",        "timeout": 2 }] }],
       "SubagentStart":      [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook SubagentStart",      "timeout": 2 }] }],
       "SubagentStop":       [{ "hooks": [{ "type": "command", "command": "/ABS/target/release/switchboard-hook SubagentStop",       "timeout": 2 }] }]
     }
   }
   ```

   Add a `SessionStart` row too if you want to see `/clear` restart the
   watcher. Then, with the daemon running in one shell and `sbr-watch.py` in
   another, start the session in a third, in the default permission mode so
   `PermissionRequest` fires:

   ```sh
   SWITCHBOARD_RUST_SOCKET=$XDG_RUNTIME_DIR/sbr.sock claude --settings ~/.cache/sbr-hooks.json
   ```

   Claude spawns hooks with the session's environment, so setting the socket
   variable on the command line is enough for every hook to find the daemon.
   `--settings` adds a settings source; your global hooks still fire, and the
   two daemons never see each other's socket. What to expect in the watcher,
   each as a `phase / lamp` pair, and each also a `shown` row in the history
   file:

   - Your first prompt: `UserPromptSubmit` arrives carrying the forwarder's
     parent pid; `resolve` walks `switchboard-hook -> sh -> claude` to the
     tracked ancestor, and the agent goes `idle / green` to
     `working / green`, reason `thinking`.
   - Ask it to run a `Bash` command it has to ask you about: `PreToolUse`
     gives `running 1 tool call(s)`, then `PermissionRequest` gives
     `attention / red`, `wants approval to run Bash`, at once: a hook is
     `Confirmed`, and a confirmed escalation never waits.
   - Approve it: `PostToolUse` settles the call and it drops back to
     `working / green`.
   - Press Escape during a long tool: `PostToolUseFailure` with
     `is_interrupt` ends the turn as `Interrupted`, and the agent shows
     `stopped / amber`, `stopped by you, work unfinished`. The `Stop` that
     follows does not erase that: a second close of a finished turn may
     explain it, never overwrite it.
   - End of a normal turn: `Stop` gives `idle / green`.

   `blocked` is not on this list on purpose. It is the amber phase for a
   usage limit or an unrecoverable error, not for a permission prompt.

6. **Fallback when step 5 is not possible** (no spare session, or a machine
   whose hooks are already spoken for). Replay a captured hook sequence into
   the socket, aimed at one tracked agent. The frames are the ones
   `decode_request` accepts: `cmd`, `event`, `pid`, `session_id`, plus the
   fixture's own fields. Save this as `sbr-replay.py` and run it from the
   repository root, with the pid of an agent that `sbr-watch.py` shows as
   `idle` and that you leave alone for the next twenty seconds:

   ```python
   #!/usr/bin/env python3
   """Replay a captured hook sequence into the daemon, aimed at one tracked agent.

   usage: python3 sbr-replay.py SOCKET PID     (run from the repository root)

   Each frame is what `switchboard-hook` would have sent, minus the scrub: the
   daemon hashes a raw `tool_input` itself when a frame arrives unscrubbed, and
   that hash is how the PermissionRequest joins the PreToolUse call before it.
   """
   import json
   import socket
   import sys
   import time
   from pathlib import Path

   FIXTURES = Path("tests/fixtures/claude/hooks")
   STRIP = ("prompt", "tool_response", "last_assistant_message", "message",
            "custom_instructions", "requested_schema")

   # (event, fixture file or inline payload, seconds to wait after, what to expect)
   STEPS = [
       ("UserPromptSubmit", "user_prompt_submit.json", 3, "working / green"),
       ("PreToolUse", "pre_tool_use_bash.json", 3, "working, running 1 tool call(s)"),
       ("PermissionRequest", "permission_request.json", 5, "attention / red, wants approval to run Bash"),
       ("PostToolUse", {"hook_event_name": "PostToolUse", "tool_name": "Bash",
                        "tool_use_id": "toolu_01ABC", "duration_ms": 12}, 3, "working / green, thinking"),
       ("Stop", "stop_bare.json", 0, "idle / green"),
   ]


   def ask(path, frame):
       sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
       sock.connect(path)
       sock.sendall((json.dumps(frame) + "\n").encode())
       reply = sock.makefile("r").readline()
       sock.close()
       return json.loads(reply)


   path, pid = sys.argv[1], int(sys.argv[2])
   agents = ask(path, {"cmd": "list"})["snapshot"]["agents"]
   agent = next(a for a in agents if a["pid"] == pid)
   session = agent["session_id"] or "replay"
   print(f"aiming at {agent['name']} (pid {pid}, session {session}, phase {agent['phase']})")

   for event, payload, pause, expect in STEPS:
       if isinstance(payload, str):
           payload = json.loads((FIXTURES / payload).read_text())
       frame = {k: v for k, v in payload.items() if k not in STRIP}
       frame.update({"cmd": "hook", "event": event, "pid": pid, "session_id": session})
       print(f"{event:<18} {ask(path, frame)}   expect: {expect}")
       time.sleep(pause)
   ```

   Every reply is `{"ok": true}`, and the watcher shows the agent go
   `idle -> working (thinking) -> working (running 1 tool call(s)) ->
   attention / red -> working (thinking) -> idle`, one block per step, with
   a matching `shown` row per step in the history file. The `PostToolUse`
   payload is inline because no captured fixture settles `toolu_01ABC`. The
   `pid` is the agent's own, so `resolve` reads the real `/proc` and finds
   it as the first ancestor; the `session_id` is rewritten to the agent's so
   the frames route to it either way. Only the `PermissionRequest` join
   differs from a live run: here the daemon hashes `tool_input` itself,
   where a live forwarder would have hashed it in `scrub` and sent
   `tool_input_hash`. To exercise that path too, pipe a fixture through the
   forwarder with the session rewritten:

   ```sh
   jq -c --arg s "$SESSION" '.session_id = $s' tests/fixtures/claude/hooks/stop_bare.json \
     | SWITCHBOARD_RUST_SOCKET=$XDG_RUNTIME_DIR/sbr.sock ./target/release/switchboard-hook Stop
   ```

   The forwarder's parent is then your shell, so `resolve` finds no tracked
   ancestor and the frame routes by session alone.

## Stretch: the plan's open edges

Each of these is a real gap in the golden, easiest first. The first six are a
couple of hours each; the last two are weekend items.

- **`subagentStatusLine` → `Snapshot::Subagents`.** The panel payload
  adapter, forwarded by a new `switchboard-hook subagent-statusline` arm. A
  pure adapter in the T03 style with a documented payload; the most
  self-contained item here.
- **Limit reset expiry.** A `Rejected` limit stays `Blocked` after
  `resets_at` passes until a new snapshot arrives. `tick` has no wall clock.
  Design it: pass `SystemTime` into `tick`, or have the core convert
  `resets_at` into an `Instant` deadline on arrival? T10 and T11 material.
- **A late replayed `TurnStarted` after `Stop`.** Transcript polling means a
  prompt line can land after the `Stop` hook for the same turn, and it would
  restart a finished turn until staleness ends it. The fix needs memory of
  the last ended `TurnKey`. Where does that belong in the model? Needs step
  5 working to reproduce.
- **`switchboard explain <pid>`.** A ring buffer of the last N inputs per
  agent, re-folded step by step to print why the lamp is the colour it is.
  The buffer goes in the core you wrote in T11.
- **Transcript replacement** (T14's stretch, if you skipped it). T13's
  kata 4 made real: a transcript deleted and recreated leaves `poll_once`
  reading a dead inode forever. Compare the open file's `MetadataExt::ino`
  against the path's each tick, or every Nth tick, and reopen at the tail
  when they differ. Test it with a rename over the watched file.
- **inotify instead of polling** (weekend). Add `rustix` and replace the
  watcher's ticker with `AsyncFd` on an inotify descriptor. Readiness-based
  I/O the course has not touched: an `EAGAIN` loop, `clear_ready`, and an
  overflow protocol where `IN_Q_OVERFLOW` means re-read. Reading: the
  `tokio::io::unix::AsyncFd` docs and `inotify(7)`.
- **pidfd deathwatch** (weekend). `pidfd_open` plus `AsyncFd`, with one task
  per agent. `ESRCH` means already dead, and `EINTR` is never death. Then the
  scanner stops emitting `Died`. Reading: `pidfd_open(2)` and the same
  `AsyncFd` docs.

## What comes after

The course is Claude-only and hooks-only until this task is graded. The
next material is the Codex adapter on the `codex` branch: a second
`ingest::codex::hook` whose `PermissionRequest` carries a `tool_use_id` and
`Ownership::Unknown`, which exercises every grey path in the fold.
