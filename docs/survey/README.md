# Survey index

Read-only investigations of the Go daemon (`~/Projects/switchboard`), the
prior Rust attempt (`~/Projects/agent-watcher`), the installed Claude Code and
Codex binaries, and this machine's transcript corpus. All written 2026-09-10
and 2026-09-11. The synthesis is `../00-rewrite-plan.md`; the concurrency
argument is `../01-concurrency-design.md`.

Live reference with captured examples of every signal:
https://claude.ai/code/artifact/3e90d067-8f18-40cd-9217-51018bb323c2

| # | File | What it covers | Headline |
|---|---|---|---|
| 01 | `01-daemon-core.md` | `cmd/switchboard`, `internal/state`, `internal/rpc` | One mutation path with a JSON change key as the publish gate; three fence mechanisms; two dead code regions in production |
| 02 | `02-discovery-lifecycle.md` | `/proc` reads, classification, pidfd, liveness sweep | Classification is one pure predicate; liveness is three-valued; key on `(pid, starttime)` to dissolve the seen-set |
| 03 | `03-providers-status.md` | Claude and Codex observers, agent graph, hook handling | The full status transition tables; per-writer prompt sets; the transcript is a falsifier of red, never a source |
| 04 | `04-navigate.md` | WM and terminal seams, the tty→pane→window join, focus/cycle/attention | Marker join `[sbw:gui:window]`; ambiguity returns nothing; Hyprland event addresses lack `0x` |
| 05 | `05-clients-renderers.md` | `switchboard-ctl` (22 subcommands), waybar broker, TUI, polybar, systemd | Hook forwarder is the privacy boundary; three renderer binaries share one loop; frozen output contracts listed |
| 06 | `06-history-federation-config.md` | History JSONL, timeline, federation over ssh, the full config surface | Eight scattered history write sites; federation never proven live; complete env/flag/config table |
| 07 | `07-design-history-and-decisions.md` | Chronology, 36 decisions, 30 hazards, contracts, doc staleness | Rewrite docs target a branch that never merged; memory sampling deleted; H9 recovery lost on the graph path |
| 08 | `08-agent-watcher-postmortem.md` | The 67k-LOC prior Rust attempt | Builds and passes tests; death watch and supervisor never wired; ~1,500 lines worth lifting; lessons |
| 09 | `09-signal-fixtures.md` | Verbatim fixtures for every input shape, cited to the Go repo | Hook payloads, RPC frames, Codex JSON-RPC, Hyprland/i3/wezterm/tmux wire formats, `state.golden.json` |
| 10 | `10-transcript-facts.md` | Everything a transcript can tell a daemon (2,374 files) | `stop_hook_summary` is the exact turn end; `origin.kind` says who started a turn; ~50 ms write latency; dedupe usage by `requestId` |
| 11 | `11-workflows-and-teams.md` | Subagents, workflow runs, agent teams, on disk and in hooks | Kind from `taskKind`/path; journal `failed` is unhandled in Go; run end exact via task-notification pairing; idle team ≠ delegating |
| 12 | `12-waiting-sessions.md` | Sessions parked on monitors, background Bash, loops, crons | `Stop` carries `background_tasks[]`/`session_crons[]`; a `waiting` status; per-kind caps from measured gaps |
| 13 | `13-tail-sentinels.md` | Usage limits, API errors, auth failures in the transcript tail | One shape (`isApiErrorMessage`) with epoch `resetsAt`; `Stop` never fires on it, `StopFailure` does; a `Block` axis on the root |
| 14 | `14-hook-events-verified.md` | Every hook event in Claude 2.1.268 and Codex 0.154, field sets from the binaries | Claude emits 23+ events, Codex 12; recommended v1 hook set; 13 behaviours to measure live |

Reading order for someone new: 07 (history), 03 (the rules), 14 (what the
hooks now give), 10–13 (what the transcript now gives), then the plan.
