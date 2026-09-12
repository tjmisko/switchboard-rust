# T13 — Capstone: run it on your own agents

**Core: 30m · Stretch: open-ended**

## Core: see it work

1. Delete `#![allow(unused)]` from `lib.rs`. Fix what it was hiding. Get
   `cargo clippy --all-targets` clean.
2. `cargo build --release`.
3. Run the daemon against the real machine, on its own socket so it can't
   collide with the Go daemon:
   ```sh
   SWITCHBOARD_RUST_SOCKET=$XDG_RUNTIME_DIR/sbr.sock ./target/release/switchboard
   ```
   Within a second it should discover every running `claude` and replay each
   transcript tail.
4. In another shell, list and subscribe:
   ```sh
   printf '{"cmd":"subscribe"}\n' | socat - UNIX-CONNECT:$XDG_RUNTIME_DIR/sbr.sock
   ```
   (or `python3 -c` with `socket.AF_UNIX` if `socat` isn't installed).
5. Point **one** Claude session's hooks at the forwarder without touching your
   global settings. Start it with `claude --settings <file>`, where the file
   registers `switchboard-hook <Event>` for `UserPromptSubmit`, `PreToolUse`,
   `PermissionRequest`, `PostToolUse`, `PostToolUseFailure`, `Stop`,
   `StopFailure`, `SubagentStart` and `SubagentStop`. Set
   `SWITCHBOARD_RUST_SOCKET` in that session's environment. Then watch the
   chip go from green to red and back as you approve a `Bash` call, and to
   amber when you press Escape.

## Stretch: the plan's open edges

Each of these is a real gap in the golden, sized from a couple of hours up to
a weekend.

- **Limit reset expiry.** A `Rejected` limit stays `Blocked` after `resets_at`
  passes until a new snapshot arrives. `tick` has no wall clock. Design it:
  pass `SystemTime` into `tick`, or have the core convert `resets_at` into an
  `Instant` deadline on arrival?
- **A late replayed `TurnStarted` after `Stop`.** Transcript polling means a
  prompt line can land after the `Stop` hook for the same turn, and it would
  restart a finished turn until staleness ends it. The fix needs memory of the
  last ended `TurnKey`. Where does that belong in the model?
- **inotify instead of polling.** Add `rustix` and replace the watcher's
  ticker with `AsyncFd` on an inotify descriptor. Overflow means re-read.
- **pidfd deathwatch.** `pidfd_open` plus `AsyncFd`, with one task per agent.
  `ESRCH` means already dead, and `EINTR` is never death. Then the scanner
  stops emitting `Died`.
- **`subagentStatusLine` → `Snapshot::Subagents`.** The panel payload
  adapter, forwarded by `switchboard-hook subagent-statusline`.
- **Codex hooks.** A second `ingest::codex::hook` adapter. `PermissionRequest`
  has a `tool_use_id` and `Ownership::Unknown`, which exercises every grey
  path in the fold.
- **`switchboard explain <pid>`** (open item C1): a ring buffer of the last N
  inputs per agent, re-folded step by step to print why the chip is the
  colour it is.
