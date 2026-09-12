# Rust golden vs Go switchboard: parity audit

**Bottom line:** the golden gets back about a third of what Go does for the user. It does much better on detection (knowing which agent wants you) than on delivery (showing it to you and taking you there).

- **Detection: about 64% weighted.** The Claude main-thread red/green logic is solid, and in several places better than Go.
- **Delivery: about 7% weighted.** There is no bar, no TUI, no focus or jump, no Hyprland or wezterm, no Codex, and no Go-compatible `state.json`.
- **It can't replace Go today.** Nothing would show up on screen, and the cutover config it needs isn't installed.

I didn't modify either repo. All 261 tests on `golden` pass (`cargo test --offline`). Paths below are relative to `~/Projects/switchboard` (Go) and `~/Projects/switchboard-rust/.worktrees/golden` (Rust).

**Survey drift:** Go gained four commits on 2026-09-11, after the surveys were written:
- `internal/pricing` and `ctl pricing`
- `ctl display` with chips and circles modes
- Codex rollout usage
- `barview` renamed to `sessionview`

`main.go` cites in the surveys are now about 27 lines off. The cites below were re-checked with `rg`.

---

## 1. Feature parity matrix

Status key: **R** recovered · **I** improved · **P** partial · **M** missing · **D** intentionally dropped or deferred (per plan §2). W is the weight explained in §2.

### A. Discovery and liveness
| # | Capability | Go | St | Rust location / reason | Notes | W |
|---|---|---|---|---|---|---|
| A1 | Find Claude processes in `/proc` every 1 s | `internal/discovery/discovery.go:42-89,105`; `cmd/switchboard/main.go:265` | R | `src/proc.rs:106-124,173-185`; `src/daemon/scanner.rs:26-50` | Same comm/exe/daemon/mcp rules | 3 |
| A2 | Find Codex processes (argv parser) | `discovery.go:190-273` | M | `proc.rs:107` only accepts comm `claude` | | 2 |
| A3 | Track `claude -p` runs, marked headless | `discovery.go:124-134`; `main.go:220-228` | M | `proc.rs:120-122` rejects them | These rows just vanish | 1 |
| A4 | pidfd death edge | `internal/osproc/source_linux.go:68-122`; `main.go:254` | P | `scanner.rs:54-67` compares consecutive sweeps | Up to 1 s latency; no kernel handle | 2 |
| A5 | Exactly one `session_end` per death | `main.go:347-371` | R | `scanner.rs:54-67`; `reduce/event.rs:28-36` | | 2 |
| A6 | A failed read is not a death (L4c) | `main.go:371` (`sessionDead`) | M | `proc.rs:146-154`: any failed read drops the row, so it dies | Comes back as a new grey agent | 1 |
| A7 | PID-reuse fence | StartedAt (discovery time) | I | `model.rs:123-128` (kernel start ticks) | | 1 |
| A8 | Zombies | Rediscovered (survey 02 §6.2) | I | `proc.rs:147` rejects state `Z` | | 1 |
| A9 | Hook attribution: ppid walk that stops at an untracked agent | `internal/rpc/rpc.go:1407-1422` | P | `daemon/rpc.rs:143-161` never stops early | A nested `claude -p` resolves to its parent (see §4.11) | 2 |
| A10 | tty capture | `internal/proc/proc.go:127-138` | M | Read at `proc.rs:167`, then dropped by `reduce/snapshot.rs:141-162` | | 1 |
| A11 | Ctrl-Z suspended flag, suspend/resume history | `main.go:993-1048`; `proc.go:103` | M | `proc.rs` uses the state field only to reject zombies | | 1 |
| A12 | WM window-closed triggers a liveness check | `main.go:770` | M | No WM | | 1 |

### B. Status derivation
| # | Capability | Go | St | Rust location / reason | Notes | W |
|---|---|---|---|---|---|---|
| B1 | Red on main-thread PermissionRequest | `internal/provider/claude/observer.go:481-499` | R | `ingest/claude/hook.rs:117-136`; `event.rs:368-430`; `model.rs:1237-1265` | | 3 |
| B2 | Red (user_input) on AskUserQuestion / ExitPlanMode | `observer.go:587-588,1140` | R | `hook.rs:37-43,95-106` | | 3 |
| B3 | Red for a subagent/teammate prompt, owned per writer | `observer.go:481-499,526-558` | P | `event.rs:131-148,385-393,491-501` | Root `Stop` can't clear it (good). But any PostToolUse or PermissionDenied from the same subagent clears its only gate (`model.rs:888`, `lift_subagent`). No subagent-transcript check. | 3 |
| B4 | Parallel prompts from one writer stay red until each is answered | `observer.go:523-545` | P | Main thread: per-call gates, `event.rs:315-365` | Not for subagents. With the current restricted PreToolUse matcher, `lift_sole_unjoined` (`event.rs:361-364`) can clear the wrong prompt. | 2 |
| B5 | Approval clears red | `observer.go:521-545` | R | `event.rs:351-358`, matched by tool_use_id | No fanout-floor delay | 3 |
| B6 | Decline clears red | `observer.go:1271-1273`; `internal/transcript/transcript.go:650-676` | I | `transcript.rs:115-134`; 200 ms poll (`daemon/watcher.rs`) | Go waits for the 5 s Observe tick | 3 |
| B7 | Interrupt at a prompt clears red | `observer.go:1256-1258` | I | `transcript.rs:87-97`; `event.rs:276-293` | | 2 |
| B8 | Working (green) | `observer.go:508-517,560-564,1604-1606` | R | `event.rs:110-127,315-348`; `transcript.rs:99-113,161-171` | | 3 |
| B9 | Idle shown orange | `observer.go:568-574`; `internal/sessionview/sessionview.go:79` | P | Tracked as a phase; lamp is green by design (`model.rs:988-995`, `docs/04-cost-model.md` §6) | `"phase":"idle"` is still published | 3 |
| B10 | Delegating | `internal/agentgraph/reduce.go:51-64`; `internal/fanout/observer.go:147-278` | P | Hook-created rows only, `event.rs:180-218`; `model.rs:1214-1221` | Needs SubagentStart/Stop hooks, which aren't in `~/.claude/settings.json`. Rows never expire. Kind is always `Task`. | 2 |
| B11 | Grey when never observed or evidence is stale | `reduce.go:9-30`; `agentgraph/types.go:163-168` | P | `model.rs:1209-1212` (only "never observed") | Evidence never goes stale | 2 |
| B12 | Interrupt during generation | Marker on 5 s tick, then idle: `observer.go:1607-1609` | I | `interruptedMessageId` → Stopped (amber), immediate | | 2 |
| B13 | API error or usage limit | Go stays green (survey 13 §0.3) | P | `hook.rs:201-212` (StopFailure); `transcript.rs:142-155,188-209` → Blocked | Fixes the per-turn case, but account limits never expire (§4.6) | 2 |
| B14 | Stale-prompt backstops (30 s TTL, 30 min cap) | `observer.go:1260,1278` | P | `reduce/mod.rs:45` for unjoined prompts only; gates on known calls never expire (`tick.rs:67-79`) | | 1 |
| B15 | H9 idle-title recovery | `main.go:1476-1490` (legacy path only) | M | — | Go's graph path lost it too | 1 |
| B16 | status_since anchoring (H1/H7/H8) | `rpc.go:926-950` | P | Time of `settle`, `model.rs:1073-1077` | | 1 |
| B17 | focused / suspended / headless / remote / unnavigable chip classes | `sessionview.go:27-55` | M | `Flags` is unused (`model.rs:1110`); `focused` is always false | | 2 |
| B18 | Fanout floor and transcript call-id latch | `observer.go:539,1322-1452` | D | Replaced by joining on tool_use_id (`event.rs:420-429`) | Needs PreToolUse registered with no matcher | 1 |

### C. Hook handling per event
| # | Event | Go | St | Rust | Notes | W |
|---|---|---|---|---|---|---|
| C1 | SessionStart, `/clear` rotation | `observer.go:503-507`; `rpc.go:756-759` | R | `hook.rs:66-77`; `event.rs:80-105` | | 1 |
| C2 | UserPromptSubmit | `observer.go:508-520` | P | `hook.rs:79-93` | Ignores who sent it: an `agent_id` hook starts the root's turn | 1 |
| C3 | PreToolUse | Stages identity only: `observer.go:476-479` | I | `hook.rs:95-115` records every call | | 1 |
| C4 | PermissionRequest | `observer.go:481` | R | `hook.rs:117-136` | Weighted 0 (same as B1) | 0 |
| C5 | PostToolUse | `observer.go:521-567` | R | `hook.rs:152-164` | | 1 |
| C6 | Stop | `observer.go:568-578` (child vs root) | P | `hook.rs:181-199`; `event.rs:129` | Ignores who sent it, so it ends the root turn. Adds a background-waits snapshot. | 1 |
| C7 | SubagentStart / SubagentStop | `observer.go:579-583` (triggers a rescan) | P | `hook.rs:214-232` | Creates/removes rows directly; no directory backfill | 1 |
| C8 | Forwarder: strip content, hash input, never fail the agent | `cmd/switchboard-ctl/main.go:612-806` | R | `ingest/forward.rs:13-57`; `bin/switchboard-hook.rs:20-42` | FNV hash, not sha256; new binary name | 2 |

### D. Transcript facts
| # | Capability | Go | St | Rust | Notes | W |
|---|---|---|---|---|---|---|
| D1 | Finding the transcript | Hook's `transcript_path` (survey 03 §5.1) | P | `paths.rs:29-33`; `daemon/effects.rs:35-36` build it from the agent's mutable cwd | The forwarded `transcript_path` is ignored | 2 |
| D2 | Tail reads: torn lines, 128 KiB window | `transcript.go:364-407` | I | `tail.rs:74-150`; `watcher.rs:39-116` | 200 ms poll | 2 |
| D3 | Turn end | `transcript.go:1236-1257` | P | `transcript.rs:173-181`: any block with `end_turn` | Fires up to 32 s early (survey 10 §4) | 1 |
| D5 | Skip local-command and meta lines | `transcript.go:132` | R | `transcript.rs:48-59` | | 1 |
| D6 | Per-request usage → `usage_sample` | `cmd/switchboard/fanout.go:152-189`; `internal/transcript/usage_tracker.go:184` | P | `usage.rs:80-114` is a library nothing calls | | 1 |
| D7 | Session name/label | `internal/label/label.go:42-103,320-340`; `fanout.go:95-104` | P | `hook.rs:75`; `paths.rs:45-52` | Only `session_title` or the cwd basename | 2 |
| D8 | Subagent transcripts and launch acks | `internal/transcript/subagents.go:350-390`; `transcript.go:263-317` | M | — | | 1 |

### E. Subagents, workflows, teams
| # | Capability | Go | St | Rust | W |
|---|---|---|---|---|---|
| E1 | `subagents/` directory scan | `fanout/observer.go:147-278`; `subagents.go:257` | M | `DirScan` is declared (`model.rs:862`) and never produced | 2 |
| E2 | `in_flight_subagents` and agent-graph nodes on the wire | `internal/state/state.go:188-325`; `internal/state/agent_graph.go:34-98` | M | `view.rs:26-38` (a count inside the reason text only) | 1 |
| E3 | Workflows and `workflow_start/stop` history | `fanout/observer.go:301-434`; `internal/transcript/workflows.go:81-189` | M | — | 1 |
| E4 | Exactly-once `subagent_spawn/stop` history | `fanout/observer.go:512-534` | M | — | 1 |
| E5 | Blocked-writer names in the tooltip | `label.go:241-270`; `internal/waybarchip/waybarchip.go:484` | P | `view.rs:188-191` shows the raw id | 1 |
| E6 | Agent tree in TUI/tooltip | `cmd/claude-tui/main.go:369` | M | — | 1 |

### F. Codex
| # | Capability | Go | St | Rust | W |
|---|---|---|---|---|---|
| F1 | `codex-hook` forwarder with TTY hints | `cmd/switchboard-ctl/main.go:138-142,709-746` | M | No such command in `frame.rs:60-85` | 2 |
| F2 | Hook reducer: request_user_input red, approval red after 30 s grace | `cmd/switchboard/codex_hook_transitions.go:873-913,936-1072,1331-1358` | M | The unowned-gate fold exists (`model.rs:1269-1294`) but nothing feeds it | 3 |
| F3 | app-server observer | `internal/provider/codex/observer.go:426,566,896` | D | Plan §2 | 1 |
| F4 | Display-name generation | `cmd/switchboard/agent_observation.go:1108` | D | Plan §2 | 1 |
| F5 | Rollout usage telemetry | `internal/provider/codex/observer_rollout.go:10-81` | M | — | 1 |

### G. Navigation and focus (all missing)
| # | Capability | Go | St | W |
|---|---|---|---|---|
| G1 | `focus-session`: WM raise, then pane activate | `rpc.go:403,596-627` | M | 3 |
| G2 | Attention ring | `cmd/switchboard-ctl/main.go:480` | M | 3 |
| G3 | `cycle next|prev` | `cmd/switchboard-ctl/main.go:447`; `internal/sessionview/navigation.go:13` | M | 2 |
| G4 | `pick` rows for fzf/rofi | `cmd/switchboard-ctl/main.go:418` | M | 2 |
| G5 | Focused bit and `focus` history | `main.go:1131-1151` (Rust: nothing produces `Placements`) | M | 2 |
| G6 | Selector grammar | `cmd/switchboard-ctl/main.go:246` | M | 1 |
| G7 | Chip order (workspace, started_at) | `state.go:1124` (Rust uses pid order, `view.rs:41-60`) | M | 1 |

### H. Terminal and WM integration
| # | Capability | Go | St | W |
|---|---|---|---|---|
| H1 | Hyprland IPC and Lua no-warp focus batch | `internal/hyprland/hyprland.go:44-161` | M | 3 |
| H2 | Hyprland events, `0x` address fix, layout debounce | `internal/wm/hyprland.go:59-105`; `main.go:667-760` | M | 2 |
| H3 | wezterm mux discovery, `cli list`, activate-pane | `internal/wezterm/wezterm.go:52-206` | M | 3 |
| H4 | `[sbw:]` marker join and spinner-stripped titles | `internal/mapping/mapping.go:258-272`; `internal/panetitle/panetitle.go:17` | M | 2 |
| H5 | Capabilities block and backend auto-detect | `internal/detect/detect.go:37`; `state.go:799` | M | 1 |
| H6–H9 | tmux, sway/i3, X11, OSC pane-bind | `internal/terminal/tmux.go`; `internal/wm/i3.go:53`; `internal/wm/x11.go:126`; `internal/panebind/osc.go:43` | D | 1 each |

### I. Outputs
| # | Capability | Go | St | Rust | Notes | W |
|---|---|---|---|---|---|---|
| I1 | `state.json` schema v3 | `state.go:1339-1417`; `internal/state/display_name.go:8` | P | `view.rs:65-94` writes `schema:"switchboard-rust/1"` to `~/.cache/switchboard-rust/` (`paths.rs:111`) | 11 fields per agent. No wezterm, hyprland, claude/codex, agent_graph or capabilities blocks. No Go consumer can read it. | 3 |
| I2 | Atomic write; publish only on change | `state.go:974,1339` | R | `daemon/publish.rs:17-41`; `daemon/core.rs:80-89` | Plus 250 ms coalescing | 1 |
| I3 | Waybar publisher (slots, pidfd signal, F8) and chip render | `cmd/switchboard-ctl/bottombar.go:242-588`; `waybarchip.go:41-521` | M | — | | 3 |
| I4 | Display modes (chips/circles) | `cmd/switchboard-ctl/display_linux.go:38` | M | — | | 1 |
| I5 | claude-tui | `cmd/claude-tui/main.go:31` | M | — | | 1 |
| I6 | Polybar | `cmd/switchboard-polybar/main.go:39` | D | — | | 1 |
| I7 | "Tracker down" chip | `waybarchip.go:139` | M | — | | 1 |

### J. RPC commands
| # | Command | Go | St | Rust | W |
|---|---|---|---|---|---|
| J1 | `list` | `rpc.go:384` | P | `daemon/rpc.rs:94` (new schema) | 2 |
| J2 | `subscribe` | `rpc.go:397` | P | `rpc.rs:96-107` | 1 |
| J3 | `list-all` / `subscribe-all` | `rpc.go:387,400` | M | Rust replies `unknown command:`; Go's bottombar fallback matches the exact string `unknown cmd:` (`bottombar.go:725`) | 2 |
| J4 | `hook` | `rpc.go:451,1005` | R | `rpc.rs:74-82` | 0 |
| J5 | `focus` / `focus-session` | `rpc.go:390,403` | M | Same as G1 | 0 |
| J6 | `pane-bind` / `pane-state` / `announce-bindings` | `rpc.go:415-450` | D | — | 0 |
| J7 | `agent-diagnostics` | `rpc.go:454` | M | — | 1 |
| J8 | `activity idle|active` | `rpc.go:460,1338` | M | — | 1 |

Rust adds one command Go doesn't have, `statusline` (`frame.rs:75`).

### K. History and timeline
| # | Capability | Go | St | Rust | Notes | W |
|---|---|---|---|---|---|---|
| K1 | Opt-in config, retention, privacy tiers, local-day files | `internal/history/config.go:87`; `internal/history/sink.go:65-378`; `internal/history/history.go:261` | P | `daemon/history.rs:12-31`; `history.rs:227` | Always on, no retention, cwd always written, UTC day files, new directory | 2 |
| K2 | `session_start` / `session_end` | `main.go:246,352` | R | `history.rs:80-101` | | 1 |
| K3 | `transition` records | `agent_observation.go:581`; `rpc.go:912` | P | `history.rs:102-106` writes `shown{from,to,held_ms}` | Go's timeline can't parse it | 2 |
| K4 | Other kinds: suspend, focus, activity, label, usage, subagent, workflow, agent_state | `history.go:33-63` | M | — | | 1 |
| K5 | `timeline --json` for the dashboard | `cmd/switchboard-ctl/timeline.go:35`; `internal/history/timeline.go:356,779` | M | — | The plan's "keep Go's timeline" only works if Rust writes Go's schema | 2 |
| K6 | `history path|tail|stat|purge|calibrate` | `cmd/switchboard-ctl/history.go:27` | M | `history.rs:206` is a reader library with no CLI | | 1 |

### L. Pricing and usage
| # | Capability | Go | St | Rust | W |
|---|---|---|---|---|---|
| L1 | Price catalog fetch and refresh; `ctl pricing` | `internal/pricing/manager.go:55-159`; `main.go:119`; `cmd/switchboard-ctl/pricing.go:24` | M | Only `parse_price` (`usage.rs:139`) | 1 |
| L2 | Cost estimate | `internal/pricing/calculate.go:15-270` | P | `usage.rs:116-190`, not wired to anything; 4 token buckets and a `[1m]` suffix fallback only | 1 |

### M. Federation
| # | Capability | Go | St | Rust | W |
|---|---|---|---|---|---|
| M1–M3 | `-remote` SSH pull, `remote-stream`, aggregate view and navigation | `internal/remotestate/manager.go:188-228`; `internal/remotestate/frame.go:252`; `internal/federation/view.go:330`; `internal/federation/navigator.go:79` | D | Type definitions only (`model.rs:1726-1806`) | 1 each |

### N. Config and CLI
| # | Capability | Go | St | W |
|---|---|---|---|---|
| N1 | Daemon flags | `main.go:86-96` (Rust `main.rs` takes no arguments; env vars only, `paths.rs:65-115`) | M | 1 |
| N2 | `ctl list [--json]` / `status` | `cmd/switchboard-ctl/main.go:154,408` | M | 2 |
| N4 | `ctl diagnose` (decision log lines) | `cmd/switchboard-ctl/diagnose.go:31` | M | 1 |
| N5 | `ctl name` and `projects.json` | `cmd/switchboard-ctl/main.go:808`; `internal/projectname/projectname.go:155` | M | 1 |
| N6 | `ctl seed-bench` | `cmd/switchboard-ctl/seedbench.go:39` | D | 1 |
| N7 | `-version` | `cmd/switchboard-ctl/main.go:37` | M | 1 |
| N8 | systemd units and deploy script | `systemd/`, `scripts/deploy` | M | 1 |

### O. Robustness
| # | Capability | Go | St | Rust | Notes | W |
|---|---|---|---|---|---|---|
| O1 | Pending red survives a daemon restart | `state.go:1378-1449`; `main.go:424-610` | M | Nothing is persisted, and a PermissionRequest leaves no transcript line | §4.4 | 3 |
| O2 | Rebuild working/idle/interrupt/error on restart | Restore, then Observe | I | `watcher.rs:39-87` replays the tail at Inferred confidence | | 2 |
| O3 | Record deaths that happened while the daemon was down | `main.go:424-470` | P | Only live agents are found; no `session_end` for the dead | | 1 |
| O4 | Socket bind safety and SIGTERM | `rpc.go:323-340`; `main.go:112` | R | `daemon/mod.rs:107-142` | | 1 |
| O5 | Laptop sleep | Wall-clock stamps | P | `view.rs:65-69` converts monotonic Instants, which stop during sleep | Ages and timers skip sleep time | 1 |
| O6 | Drop rather than block under load | `sink.go:96` | R | `effects.rs:46-51`; `rpc.rs:129-135` | | 1 |

**New in Rust, not scored (Go has no equivalent):**
- Waiting phase from `Stop.background_tasks` (`hook.rs:320-349`)
- Elicitation shown as attention (`hook.rs:266`)
- StopFailure, PostToolUseFailure, PermissionDenied, CwdChanged, compaction events
- Statusline limits and beat (`statusline.rs`)
- Session id read from `sessions/<pid>.json` with a procStart check (`proc.rs:129`)
- Dwell before escalating (`model.rs:1054-1107`)
- A subagent-panel reducer (`snapshot.rs:73`) that nothing feeds

## 2. Headline numbers

There are 111 Go capabilities in the matrix: 16 recovered, 8 improved, 26 partial, 48 missing, 13 dropped. Recovered and improved count as 1, partial as 0.5, missing and dropped as 0.

| Measure | Result |
|---|---|
| Raw, all rows | 37/111 = **33%** (38% if dropped rows are excluded) |
| Raw, recovered/improved only | 22% |
| Weighted | 61.5/168 = **37%** (39% if dropped rows are excluded) |
| Weight-3 rows only (16) | 7.5 = 47% |
| Detection areas (A, B, C, D, O), weighted | 53.5/84 = **64%** |
| Delivery areas (G, H, I, J, N), weighted | 4/55 = **7%** |

**How I weighted:**
- **3:** the loop "an agent wants me → I see it → I get there." Red detection and clearing, working state, idle distinction, discovery, Codex red, focus and attention, Hyprland and wezterm, the bar, `state.json`, red surviving a restart.
- **2:** daily but secondary. Delegating, chip classes, labels, cycle and pick, history for the dashboard, `list`.
- **1:** peripheral. Pricing, diagnostics, other WMs, federation, flags.
- **0:** rows that repeat another row (C4, J4–J6), so they aren't counted twice.

## 3. Where Rust is right and Go is wrong

1. **False green after an API error or usage limit.** Go only listens for `Stop` (`observer.go:568-574`), which never fires on an API failure, so the chip stays green indefinitely (`docs/survey/13-tail-sentinels.md:19-22`). Rust turns StopFailure (`hook.rs:201-212`) or an `isApiErrorMessage` transcript line (`transcript.rs:142-155`) into Blocked. Caveat in §4.6.
2. **Stale red after answering a prompt.** `04-cost-model.md:119-130` (R4) names this Go's costliest bug.
   - *Go:* a decline fires no hook, so it waits for the 5 s Observe tick. An approval with subagents in flight is held by the fanout floor (`observer.go:539`). Codex approval red is re-asserted until you type (`codex_hook_transitions.go:953-993`; `docs/survey/14-hook-events-verified.md:121`).
   - *Rust:* approval clears on the PostToolUse id match (`event.rs:351-358`); a decline clears from the transcript within about 200 ms (`transcript.rs:115-134`).
3. **Esc feedback.** Go catches the interrupt marker on a 5 s tick and shows orange idle (`observer.go:1607-1609`). Rust keys on `interruptedMessageId` and `PostToolUseFailure.is_interrupt`, marks it user-caused, and shows amber immediately (`transcript.rs:87-97`, `hook.rs:166-179`, `model.rs:1073`). This is the "contradicted calm" case in `04-cost-model.md` §2–3.
4. **Pricing and `inference_geo`.**
   - *Go:* Claude transcripts report `inference_geo:"not_available"` (`docs/survey/10-transcript-facts.md:332`). Go's `selectRates` treats any value other than empty, global or default as a geography (`calculate.go:260-266`). Only a `us` multiplier exists (`source.go:185-188`), so every current Claude request comes back "inference geography price is unavailable" and the whole estimate is marked unknown (`calculate.go:44-47`). No Go test covers `not_available`.
   - *Rust:* `usage.rs` ignores the field and prices at base rates. But that code isn't wired to anything.
5. **Process identity.** Rust keys on the kernel start time (`model.rs:123`) and rejects zombies (`proc.rs:147`). Go has a zombie rediscovery loop (survey 02 §6.2).
6. **Session known before the first hook** (`proc.rs:129-160`), so the transcript can be replayed as soon as the agent is discovered.
7. **Waiting on background tasks and crons.** Go has no such state (`docs/survey/12-waiting-sessions.md:256-266`).
8. **MCP elicitation shows as attention.** Go misses it (`docs/survey/14-hook-events-verified.md:84`). Caveat: Rust doesn't handle ElicitationResult, so the red lingers until the turn ends.
9. **Anti-flap dwell and a rule that unowned gates stay grey** (`model.rs:1054-1107,1199-1203`). In Go, 53 Auto-review false-red episodes led to a 30 s grace (`docs/survey/07-design-history-and-decisions.md:142`). No Codex code exists in Rust yet to exercise this.
10. **cwd follows CwdChanged** (`event.rs:234-241`); Go records cwd once at discovery. Caveat in §4.14.

## 4. What a user would notice if Rust replaced Go today

1. **Nothing on screen.** No Waybar chips, circles, TUI or polybar. `state.json` is at a new path with a new schema. `subscribe-all` isn't recognised, so Go's publisher would show empty chips or "tracker down."
2. **No way to jump.** Chip clicks, scroll-to-cycle, and the Hyprland attention/cycle/pick bindings all stop working.
3. **Codex sessions don't appear.**
4. **A permission prompt open during a daemon restart turns green.** It shows "running 1 tool call," then idle after 10 minutes. The red never comes back (`tick.rs:65-79`, no hydration).
5. **Idle and working look the same** (both green). Go's orange "finished" colour and its idle tier in the attention ring are gone.
6. **One usage-limit hit turns every Claude chip amber, working ones included, and they stay amber after the reset.** Limits are never expired (`model.rs:1186`), and only a statusline "allowed" snapshot clears them. A restart replays the old line from the last 128 KiB of the transcript.
7. **A subagent's prompt can drop to green while it is still waiting.** If a parallel auto-approved tool in the same subagent finishes, `lift_subagent` clears the gate (`event.rs:146-148`).
8. **Delegating depends on hooks you don't have registered.** Without SubagentStart/Stop there is no delegating state at all. With them, one missed SubagentStop leaves an agent "delegating" forever.
9. **Hook config has to change.** Your `settings.json` restricts PreToolUse to `AskUserQuestion|ExitPlanMode` and doesn't register StopFailure, PostToolUseFailure, PermissionDenied or the subagent hooks. Under the current config, `lift_sole_unjoined` can clear the wrong prompt, and the Rust forwarder is a different binary with a different hash.
10. **Your status line goes blank.** `switchboard-hook statusline` prints nothing (`switchboard-hook.rs:44-52`) instead of passing through to `~/.claude/claude-status`.
11. **An agent running `claude -p` can hijack its parent's chip.** The child's SessionStart is attributed to the parent, rotates the parent's session (`event.rs:84-100`), and the parent's later hooks are dropped (`event.rs:47-49`).
12. **The dashboard has nothing for Rust days.** Rust writes `shown` records to UTC day files in a new directory, with no opt-in, retention or privacy tier.
13. **Smaller visible changes:**
    - `claude -p` runs disappear.
    - Ctrl-Z'd agents aren't greyed.
    - No focused highlight.
    - Chips are in pid order.
    - Labels lose `sessions/<pid>.json` names and project abbreviations.
14. **Some sessions lose all transcript-based detection.** If the transcript path built from cwd is wrong (a CwdChanged followed by `/clear`, or a process cwd that isn't the project), there is no decline, interrupt or error detection and no restart replay.
15. **Early "idle".** Phase can flip to idle up to 32 s before the reply finishes (`transcript.rs:173`). Only the phase text shows it.
16. **Wrong ages after laptop sleep**, and no `pricing`, `diagnose`, `-version` or daemon flags.

## 5. Shortest path to daily use, in priority order

1. **Correctness fixes before anything else.** Small, about 300 LOC with tests, roughly a day. Each prevents a wrong colour.
   - Expire rejected limits at `resets_at`.
   - Give each subagent a per-call gate map instead of one `Option<Gate>`.
   - Make TurnStarted, TurnEnded and interrupt check who sent them.
   - Stop the ancestor walk at any claude process.
   - Use the hook's `transcript_path`.
   - End turns on `Stop`/`stop_hook_summary`, not on a block's `end_turn`.
   - Separate "gone" from "unreadable" in the sweep.
   - Add a stale cap on subagent rows.
   - Make the statusline forwarder tee to the wrapped script.
   - Register the full hook set alongside the Go hooks. Config only.
2. **A Go-compatible v3 `state.json` plus `list-all`/`subscribe-all`.** Medium, about 400–600 LOC. The existing Go bottombar, display, TUI, `pick`/`cycle`/`attention` and `diagnose --observer` would then render Rust's truth without porting any renderer. It also restores the plan §7 side-by-side diff. You'd need to decide how Stopped, Blocked and Waiting map onto `idle`/`working`.
3. **Navigation.** Medium–large, about 1k LOC.
   - Hyprland request and event sockets with the Lua focus batch (~400).
   - wezterm `cli list`/activate (~250).
   - Marker join (~100).
   - A resolver task that feeds the existing `Placements` snapshot (~150).
   - A `focus-session` RPC (~100).
   Go's `ctl focus`/`cycle`/`attention` then work unchanged.
4. **Keep red across restarts.** Small–medium, about 200 LOC. Persist open human gates, restore them, and use the transcript replay only to clear them, never to create them.
5. **Codex hook path.** Medium, about 500 LOC. `codex-hook` frames, the argv classifier, and an adapter for the 12 events. One decision first: Rust's "unowned means grey" rule would regress Go's approval red after 30 s.
6. **History in Go's schema.** Small–medium, about 200 LOC. `transition` and `session_*` records with `agent` fields, local day files, the `history.json` opt-in, and focus/suspend/activity events, so Go's `timeline --json` keeps feeding the dashboard.
7. **Small gaps.** About 100 LOC. Suspended flag from stat state `T`, tracking `-p` runs as headless, `sessions/<pid>.json` names.

Can wait: subagent directory scan and workflows, pricing, federation, `diagnose` rule ids, daemon flags. Items 1–4 are the gate for running Rust beside Go. Items 1–5 are the gate for switching Go off.