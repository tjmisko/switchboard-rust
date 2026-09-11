# 07 — Design history and decisions (distilled from `~/Projects/switchboard/docs`)

Read-only distillation of the Go project's design documents, incident reports,
plans and retrospectives, checked against the code at `main` = `9ae403d`
(2026-09-02). Paths below are relative to `~/Projects/switchboard/` unless
absolute. Commit hashes are from `git log` on `main`.

Size of the thing being rewritten (measured, `cmd/` + `internal/`, excluding
`.worktrees`): **36,123 non-test LOC, 40,202 test LOC**. Both rewrite docs
quote "~39k LOC"; that figure is now non-test only and ~10% high.

---

## 1. Chronology

The repo starts as Go. The shell prototype the README credits ("growing out of
a shell prototype I hand-wrote") is not in git; the first commit `916966f`
(2026-05-28, "claude-tracker") is already a 2,020-line Go daemon for
Hyprland + wezterm + waybar with `discovery`, `proc`, `procwatch`, `hyprland`,
`wezterm`, `mapping`, `state`, `rpc`, a ctl and a per-slot waybar binary.

| Phase | Dates | Commits | What happened |
|---|---|---|---|
| **0. Appliance** | 05-28 | `916966f`–`8edbbfd` | Hyprland/wezterm/waybar tracker; auto-hiding bottom bar; hooks map straight to a status enum. |
| **1. Portability pivot, Phases 0–3, 5** | 05-28 (one day) | `e9ca068`–`ff17379` | Rename to switchboard (`a059969`); `docs/portability-plan.md`; Phase 0 test-first baseline (`b139b2d`…`abe994f`, 12 ⚠ characterization items → `docs/decisions.md`); Phase 1 seam extraction `osproc`/`terminal`/`wm` + runtime `detect` (`6b3925e`–`fcc77fd`); Phase 2 sway/i3 + X11 backends (`304d77c`, `e383e5c`); Phase 3 tmux + locator chaining (`f974d94`); Phase 5 `claude-tui` + bar recipes + README rewrite (`4a8c581`, `d63f673`); Phase 4 macOS **planned only** (`c9dbfe0`). `DONE.md` was written once, for Phase 0, and never updated again. |
| **2. Status-color model** | 05-30 → 06-23 | `8cb7868`–`9a16593` | Permission self-heal from transcript (`e536d2e`); transition decision log and idle/working self-heal (`fc8ce13`, `1977cf4`); hold red while subagents work (`2964a8c`, `cd373a9`); `!`/`/` local commands not activity (`4a8837a`); Codex discovered + hook status (`8e195b8`); `docs/status-color-state-model.md` + resolution-kind classifier + `statustune` knobs + `ctl diagnose` (`d0c83f8`–`1aeb0fd`); H1 transcript anchor (`1814abb`), then the opposite-direction idle anchor fix (`9a16593`); `docs/timing-hazards.md` (`1836bae`). |
| **3. Naming, history, fanout** | 06-24 → 06-26 | `c8c5e13`–`3fd5ef9` | Project abbreviations + display-time prefixing (`docs/session-naming.md`); usage history F1–F4 in one PR (`2975ff7`, `#30`) then v2 timeline with cost/names/focus/delegation (`204203b`–`6bfcca0`, `#31`); `docs/usage-history-plan.md` fully executed; subagent fanout Observer with `subagents/` dir scan + forward cursor + optional `SubagentStart/Stop` hooks (`8b3bcf8`–`3fd5ef9`, `#38`). |
| **4. Ghost lanes and timing races** | 07-21 → 08-03 | `55ca6ac`–`1ce1ef9` | Session id refreshed on every hook (`55ca6ac`); H7/H8 wall-clock anchor (`8024898`); H9 idle-title demotion (`fc33c1d`); ghost-lane liveness sweep + `docs/session-lifecycle-hazards.md` (`a5c554c`, `fe083be`); swimlanes grouped by session id (`731cb7f`); headless `claude -p` runs (`16af81a`–`5f068d2`); suspect-lane post-check and `history calibrate` (`b0bf3cb`–`bd50d33`). |
| **5. Memory sampling** *(later retired)* | 08-03 → 08-04 | `7ca21bd`–`679881c` | Per-session PSS/swap sampling, `memory_sample` events, `ctl memory`, 2 GB history cap. Measured lock budget recorded in `docs/decisions.md`. **Deleted 08-26** (`082da39`). |
| **6. Perf + writer model** | 08-05 | `993d709`–`c96af1a` | I/O hoisted out of the store lock, encode-once broadcast, publish suppression (`bbfab46`–`df96049`, `#54`); the 2026-08-05 subagent-permission oscillation incident, diagnosis and per-writer pending map (`f72120b`–`b55725a`, `#58`); nested `claude -p` hook mis-attribution fix (`e6b24ba`); workflow visibility (`f853c44`–`c96af1a`, `#61`). |
| **7. Rust kickoff docs** | 08-12 | (uncommitted until `f156f23`) | `docs/rust-rewrite-kickoff.md` and `-requirements-v0.2.md` written; dashboard repo scoped separately. Both amended 08-24 for the Codex no-wrapper change and not touched since. |
| **8. Launch-ack correction** | 08-13 → 08-14 | `178708e`–`426edce` | Activity state carried across midnight and capped (`178708e`, `7ba714a`); every `Agent` spawn is async and its `tool_result` is a launch ack, not a completion (`198cc8d`, `docs/async-agent-launch-ack.md`, `#75`); history repair script. |
| **9. Codex agent graph** | 08-21 → 08-24 | `30e96a4`–`59b7eda` | Ten-workstream planning packet `docs/codex-session-status/` executed in one day (`33570a7`–`342b8d2`): provider-neutral `agentgraph`, Codex app-server observer, Claude adapter, renderers, history `agent_state`; graph becomes sole authority for Claude (`784e3a8`). Shared-daemon attribution incident (`41b3f1d`); a wrapper/"slot sessions" approach (`e08fd21`) tried and **removed**; interview retrospective (`c6607cb`); red reserved for human requests (`bc36f43`); no-wrapper stdio spikes and exact child-hook fusion (`c3dfe25`–`3af93cb`, `#87`); Codex display names (`f156f23`). |
| **10. i3/Polybar, SSH federation** | 08-24 → 08-25 | `27adbcf`–`b0c6856` | `switchboard-polybar` + i3 PID hydration (`docs/i3-polybar-plan.md`, executed); SSH federation + WezTerm OSC pane binding in one commit (`24bc51a`); per-machine renderer profiles (`bd53d9b`, `docs/machine-configuration.md`). |
| **11. OOM incident, memory retirement, deploy model** | 08-26 → 08-28 | `315c569`–`6f594d9` | Seed-replay OOM kill loop; `seed-bench`, streaming seed, seed cursor (`docs/seed-replay-memory-plan.md`, fully executed, results recorded); memory sampling deleted (`082da39`); remote hysteresis PR `#90` **reverted** (`eab98a2`) after `docs/review-pr-90.md`, replaced by one bounded reconnect confirmation (`6cd30d8`); immutable release deploy (`768a88d`–`79fa768`, `#93`); memory-footprint Phase 0 + Phase 1 stacked commits (`8ef2f0d`–`886e3ee`). |
| **12. Attention ring, AskUserQuestion model, PreToolUse** | 08-31 → 09-02 | `8571016`–`9ae403d` | Bounded urgency ring (`8571016`, `4bc7bce`, `docs/attention-ring-plan.md`); AskUserQuestion reconciled gameplan Phases 1–5 (`f6df139`–`b15755d`, `docs/askuserquestion-model-plan.md`); decision log restored on the graph path (`d4fc6dc`); store publish ordering, wakeup coalescing, persistence off the fanout (`ebde783`–`3901cb9`); waybar slot renderers consolidated into `bottombar watch` and a one-process attached publisher (`761fcf6`, `28b3491`, `9ae403d`). |

### Plan execution status

| Doc | Status | Evidence |
|---|---|---|
| `portability-plan.md` | Phases 0,1,2,3,5 executed; **Phase 4 (macOS) never started**; 3.2 kitty skipped; 3.3 tmux→WM raise still deferred; 5.3 polybar/eww unverified. | `docs/phase4/README.md` "NOT STARTED"; `internal/osproc/source_darwin.go` is a stub. |
| `behavior-spec.md` | Executed (Phase 0 tests). **Stale** in §7.4: asserts Claude Code withholds a pending `tool_use` until resolved — disproved by V4 (`subagent-permission-plan.md` §9.7). | |
| `decisions.md` | 12 verdicts all resolved except #6 Monitor (still `""`). **Stale** section: "Memory sampling — the lock budget" describes a deleted feature. | `rg mem_agent_bytes internal/state` → no hits. |
| `status-color-state-model.md` | Phases A, B executed; A3 landed later as step 10 of the AUQ plan; B3 (hooks) landed; C (gray on rehydrate) never. §10's rule→knob table describes the **legacy reconciler that no longer runs for Claude** (gated by `sess.AgentGraph != nil`, `cmd/switchboard/main.go:1415`). `knobs.go` now carries **38** rule ids, not 19. | |
| `timing-hazards.md` | H1–H9 tests exist. **H9 recovery is dead on the Claude graph path** since `784e3a8`; no `IdleGlyph`/`TitleAt` reference exists in `internal/provider/claude` or `agent_observation.go`. R7 in `attention-latency-report.md` was never done. | |
| `session-lifecycle-hazards.md` | Executed (F1–F4, suspect post-check). "Pass live pid set into BuildSwimlanes" still not done. | |
| `usage-history-plan.md` | Fully executed (§9a). | |
| `subagent-fanout-detection-plan.md` | Executed with three documented corrections (meta heterogeneity, launch ack, spawnDepth). | |
| `subagent-permission-plan.md` | T1–T12 executed. Its R2 ("PreToolUse rejected on cost") is **superseded** by `pretooluse-hook-plan.md`. | |
| `askuserquestion-model-plan.md` | Phases 1–5 executed with four addenda of repairs; U1/U2 closed in §4g. | |
| `pretooluse-hook-plan.md` | Phases A–C done: matcher `AskUserQuestion\|ExitPlanMode` is registered in `~/.claude/settings.json`. Phase D live acceptance metrics not recorded. | |
| `attention-ring-plan.md` | Executed. | |
| `codex-session-status/` | Executed, then **partially superseded** (proxy, shared daemon, primary-status assumptions) by the no-wrapper docs. Marked historical. | |
| `codex-auto-review-attention.md` | Phase 1 (30 s grace, timeout-to-human retained) executed; **Phase 2 (evidence-gated red) not done.** | |
| `codex-no-wrapper-child-lifecycle.md` | Executed; #83 "final rollout capture" gate never recorded. | |
| `ssh-federation-plan.md` | Phases 0–3 implemented; **every live acceptance check listed in §12 is still marked pending.** | |
| `review-pr-90.md` | The reviewed branch was reverted wholesale (`eab98a2`); no `hold.go`, `Stale`, `-remote-hold` exist. Historical. | |
| `seed-replay-memory-plan.md` | Fully executed with measured results. | |
| `memory-footprint-plan.md` | Phase 0 done; Phase 1 tasks #4.5–#8.5 implemented (`54b077c`, `ea06c36`, `66ef22d`, `eeaf7de`, `f0a4448`, `0e531b8`) but boxes left unchecked pending live gates; Phase 2 renderer consolidation **landed** (`761fcf6`, `28b3491`) though the plan still says `[NEXT]`; #9 title rate limit **not landed** (no `EventTitleChanged`); Phase 3 lazy Codex app-server **not landed** (no `IdleShutdown`); §5 results table never filled. | |
| `i3-polybar-plan.md` | Executed. | |
| `workflow-visibility.md`, `async-agent-launch-ack.md`, `session-naming.md`, `codex-session-naming-findings.md` | Executed. | |
| `rust-rewrite-kickoff.md`, `rust-rewrite-requirements-v0.2.md` | Not executed here; see §5. | |
| `state-schema.md` | **Stale**: documents `mem_agent_bytes`/`mem_tree_bytes` (removed `082da39`); the "permission self-heal (reconciler)" and "decision log" paragraphs describe the legacy path. Otherwise current (schema_version 3, `display_name`, `pending_prompts.call_id`, `resolved_name`, `local_workspace` all exist in `internal/state/state.go`). | |
| `README.md` | Current, except the layout omits `internal/panetitle`, `internal/buildinfo`, `internal/barview` and lists nothing for the empty leftover dir `cmd/switchboard-codex` (its wrapper was removed in `f156f23`). | |

Unmerged branches at survey time: 44 (`git branch -a --no-merged main`),
including `docs/writer-state-model` (`2040a91`, the `internal/writerstate`
FSM the requirements doc tells the port to target) which was never landed.

---

## 2. Decisions register

Each row: the decision, why, and whether the reason survives a Rust rewrite.

| # | Decision | Reason | Still applies? |
|---|---|---|---|
| D-1 | **Discovery (OS process layer) is the source of truth for existence; hooks and observers only enrich.** A hook for an unknown pid creates no session. | A broken hook must never corrupt state; Switchboard must work with zero hooks. (`README`, `rust-rewrite-requirements-v0.2.md` AP7.) | Yes, foundational. |
| D-2 | **Death is observed (pidfd), never inferred**, with a per-tick liveness sweep as a backstop. Liveness is three-valued: alive / gone / unsupported. | L1–L7 ghost lanes; the darwin stub returns `ErrUnsupported` for every pid and must not end every session. (`session-lifecycle-hazards.md` §3, AP9.) | Yes. Rust: `pidfd_open` on Linux, `kqueue`/`EVFILT_PROC` on macOS. |
| D-3 | **Four-color action semantic**: red = blocked on a human, green = work happening (incl. `delegating`), orange = your turn, grey = unknown. Grey-out overlay for Ctrl-Z. | Color answers "do I need to act"; asymmetric costs (§4 of `status-color-state-model.md`). | Yes. |
| D-4 | **Asymmetric error costs**: missed RED is the worst error; stale RED second. Every guard that refuses to clear red is a "missed-RED guard" with **no tuning knob**. | Measured: a missed red is silent for minutes–hours; a stale red is loud for seconds. | Yes, and the requirements doc makes it a MUST (FR-G4). |
| D-5 | **A session is 1 + N writers** (main thread + each in-flight subagent) sharing one pid and one chip; a pending prompt is owned by the writer that raised it and released only by that writer's evidence. | 2026-08-05 oscillation incident: `tool_name` matching cleared a teammate's red. | Yes (FR-M1/M2). |
| D-6 | **Correlator strength**: `tool_use_id` (exact call) > `agent_id` (writer) > `tool_input` hash (call shape) > `tool_name` (kind). Never latch or clear on a hash mismatch; a mismatch proves nothing because `PostToolUse` carries `updatedInput`. | `claude-code-hook-schema.md` §2. | Yes; the hook payload shapes are Claude Code facts, not Go facts. |
| D-7 | **`PermissionRequest` is the only Claude hook that opens red.** `PreToolUse` (matcher-limited to `AskUserQuestion\|ExitPlanMode`) only stages an id proposal; a join miss falls back to the transcript latch. | `pretooluse-hook-plan.md` safety rule. | Yes. |
| D-8 | **Hooks AND transcript tailing, not one or the other.** Hooks are lossy (interrupt fires no `Stop`, decline fires nothing, teammate wake fires nothing); the transcript is a *falsifier* of red, never a *source* of red. | `timing-hazards.md` §1; `subagent-permission-plan.md` §9.3 ("unsound as a source, sound as a falsifier"). | Yes (FR-F1, FR-F3/F4). |
| D-9 | **Per-edge `status_since` anchoring**: into working → transcript anchor (H1), floored at the previous `StatusSince` (T4); into idle/permission → wall-clock `now` (H7/H8). | Flush-ordering races run opposite ways on the two edges. | Yes (FR-G5). |
| D-10 | **Pane title is a third signal stream** used only for positive evidence of idleness (H9), never for names, never for Codex. | Double-Esc before first token writes nothing anywhere else. | Yes — but note the Go graph path lost it (§3 H9). |
| D-11 | **Pending prompt ownership persists across restart** as `pending_writers` (key set) + `pending_prompts` (records incl. `attention`, `since`, confirmed `call_id`); hydrate may only *falsify* an entry against the writer's own transcript, never manufacture one. `Since` for the whole-file rules is re-stamped to restart. | No hook re-raises a live prompt, so a dropped owner is a permanent missed red. Two clocks on purpose (`askuserquestion-model-plan.md` §4e). | Yes (FR-L6). |
| D-12 | **Latch a call id lazily** from the writer's own unmatched `tool_use`, only when unique, only from a read taken ≥2 s after a proposal, only for calls dated within 3 s before the prompt; ambiguity is terminal. | Repairs R1/R2/R5 in the AUQ plan: the first read after `PermissionRequest` sees the *sibling's* call. | Yes; these are transcript-writer timing facts. |
| D-13 | **Decline detection keys on `toolDenialKind == "user-rejected"` or a `"User rejected tool use"` result**, never on `is_error` alone. | Corpus: 218 error results, only 14 user-rejected. | Yes. |
| D-14 | **`delegating` is a stored legacy summary value** rendered the same green as `working`; a live teammate is simply work happening. Child liveness needs positive evidence (pending/running lifecycle, active/idle runtime, or attention); topology alone contributes zero. | Q1 pure green; the false-live defect with retained `not_loaded` Codex children. | Yes. The unmerged `writerstate` FSM would delete `delegating` as a state entirely. |
| D-15 | **Children are never navigation targets.** A child agent has no tty and no window; only the root `Session` is a chip, focus selector, cycle or pick target. | `codex-session-status/README.md` locked decision 1. | Yes. |
| D-16 | **Provider-neutral three-axis graph** (runtime / attention / lifecycle) reduced by one source-neutral reducer with an explicit freshness interval `observed_at <= now < fresh_until`; an expired graph goes grey rather than freezing a confident color. | Codex and Claude expose different evidence; freshness is authority. | Yes (FR-E6/E7). |
| D-17 | **Red only for confirmed human ownership** on Codex: a mechanical `waitingOnApproval` gate is not attention; ambiguous approvals get a 30 s grace (timeout-to-human still enabled as Phase 1); structured `request_user_input` is red immediately. | Auto-review produced 53 false-red episodes on 08-24. | Yes; Phase 2 (never red without evidence) still to do. |
| D-18 | **Exact Codex attribution only**: `(pid, started_at)` from discovery + `session_id` from a trusted hook whose ancestry reaches the TUI. Never cwd, title, recency, launch order, or loaded-thread lists. No wrapper, alias, token or registration. `/clear` rotates the identity and fences late events. | Shared-daemon incident; the wrapper design was a scope failure (`codex-standard-cli-interview-retrospective.md`). | Yes (AP11, FR-S1–S3). |
| D-19 | **Disposable `codex app-server --stdio` per daemon** for topology; hooks own bounded root status and exact child edges after an id match. | The shared daemon moves thread ownership; app-server snapshots report interactive roots as `notLoaded`. | Yes, with the caveat that stdio is empirically verified on 0.149, not documented. |
| D-20 | **Codex display names** are Switchboard-owned, conversation-bound, generated from a bounded prompt + final response after the first completed turn; native `/rename` always wins; native names are never mutated. | First-prompt naming was premature; mutating native state was wrong. | Yes (FR-S7–S11). |
| D-21 | **One binary, runtime backend detection** (WM by env precedence Hyprland → sway → i3 → X11 → none; terminal by socket existence); build tags only for the OS syscall layer; every seam has a `none` backend and a force flag. | `go install` works anywhere; Linux is not one platform, the WM is. | Yes (SD5/SD6). |
| D-22 | **The join is tty-anchored**: pid → tty (kernel) → pane (terminal seam) → `(mux pid, title)` (WM seam). The WM hop returns nothing on ambiguity and retries. | The agent owns no window; only the mux does. | Yes; the requirements doc generalizes tty to an opaque `SessionAnchor` for Windows. |
| D-23 | **Titles are spinner-normalized at the terminal seam** (`internal/panetitle`) so the join, the label, the change key and the wire all see one form; but the pane's own raw `Title` is kept for H9. | Spinner rotation broke the join and drove 5–11 publishes/s. | Yes. |
| D-24 | **Publish only on observable change**: a JSON-encoded change key with clock fields (`updated_at`, `observed_at`, node `updated_at`) excluded; `fresh_until` published on a 5 s ceiling so suppression is sound; encode once per publish; federation View change-gated; renderers own their duration clocks. | 9.4 publishes/s × 11 subscribers ≈ 1.4 MB/s for nothing. | Yes. The soundness proof (`memory-footprint-phase1/02` §"soundness") is worth carrying verbatim. |
| D-25 | **No I/O inside the store lock.** Sampling, transcript reads, provider calls happen before `Apply`; the lock holds sub-microsecond work. | Measured 14–96 ms per pass would have blocked every RPC and hook. | Yes (NFR-1). |
| D-26 | **History is opt-in, append-only JSONL, one file per local day, under `$XDG_STATE_HOME`**, with `minimal` (default) / `full` privacy tiers, size-bounded before age-bounded retention (90 d / 100 MB). No SQLite (cgo, portability). | Records when/where you work. | Yes (FR-N). SQLite argument is Go-specific; a pure-Rust store is possible but the "log of record + derived index" shape stands. |
| D-27 | **The suspect post-check flags, never drops**: an unclosed lane with a final interval ≥ 4 h (subagent span ≥ 2 h) is annotated and excluded from aggregates; one `--suspect-cap` scales both. | A dropped-`Sink.Record` or torn line can lose a `session_end` at the source. | Yes (FR-N4). |
| D-28 | **`session_end` has one writer, three triggers** (pidfd, sweep, startup stale-drop) and every removal path routes through it. | L7: a bare `delete` hid a pid from the sweep forever. | Yes (FR-L1–L3). |
| D-29 | **Every subagent `Agent` spawn is asynchronous**; its parent `tool_result` is a launch ack, not a completion; completion comes from the child jsonl's `end_turn` or a stale cap. `run_in_background` is decoration. | 69/69 spawns acked, 0 with the flag. | Yes; a Claude Code fact. |
| D-30 | **Workflow runs** (`subagents/workflows/wf_*/journal.jsonl`) count as depth-1 fanout; in-flight = started − resulted − stale-closed. | Twenty agents burned tokens behind an orange chip. | Yes. |
| D-31 | **Attention ring is urgency-first and bounded**: red + orange when red exists, orange + green otherwise; already-focused rows skipped; grey/headless/unbound-remote never members; `cycle` stays unrestricted. | `$mod+A` re-focused the window you were in. | Yes. |
| D-32 | **Bar naming is display-time**: project abbreviation from git root + de-dup, so `/name` never fights it. Codex terminal titles are never labels. | Claude's live name cannot be changed externally. | Yes. |
| D-33 | **Federation is one SSH child per host streaming full snapshots; rows namespaced `(hostname, pid)`; exact focus needs a WezTerm OSC user-var binding; transport EOF triggers one bounded reconnect before rows drop.** Hysteresis with stale/held rows was **rejected** after review. | Keep the design small; the hold machine had three blocking defects. | Yes as a design; live acceptance was never recorded. |
| D-34 | **Immutable release deploy** with `current` symlink, `/proc`-verified running revision, wrong-tree/downgrade/stamp guards. | `go install` + restart reported success against the old binary. | Yes as ops practice. |
| D-35 | **Codex `Stop`/`SessionStart`/`UserPromptSubmit` pending sweeps are writer-blind**; Claude's are writer-scoped. Two prompt models, one vocabulary, no shared code. | Only the root emits those on Codex; child hooks are diverted first. Sharing code produced M1. | Yes; keep the models separate in Rust too. |
| D-36 | **Telemetry lines are permanent, fixed-order, content-free** (`publish-stats`, `fanout-seed`). | Each answers an incident that could not be diagnosed from outside. | Yes. |

---

## 3. Hazards register

| Id | Symptom | Root cause | Fix that landed | Invariant a rewrite must preserve |
|---|---|---|---|---|
| **H1** quick interrupt | Ctrl+C ~1 s after prompt leaves chip green forever. | `StatusSince` stamped at hook-processing time sits *after* the interrupt marker. | Anchor the working edge to the transcript's newest entry (`1814abb`). | Into-working edge dates from the transcript, floored at the previous `StatusSince`. |
| **H3** busy turn | A long tool run must stay green. | — | Demotion keys on the interrupt marker, never a no-activity TTL. | No activity TTL anywhere in status. |
| **H4** local command | `!bash` after idle re-greened the chip. | `<bash-stdout>` user entry read as activity. | Classifier ignores local command entries (`4a8837a`). | `!`/`/` entries are not agent activity. |
| **H7** Stop flush race | Every `Stop` re-greened next tick. | Final assistant line flushes *after* the hook; a transcript-anchored idle edge sees it as later activity. | Idle edge anchors to wall-clock `now` (`9a16593`). | Into-idle edge = `now`. |
| **H8** permission pre-prompt flush race | Red released to green in one tick while an AskUserQuestion waited 7½ min (2026-07-22). | Thinking/text/`tool_use` lines of the blocked turn flush after `PermissionRequest`. | Permission edge anchors to `now` (`8024898`). | Into-permission edge = `now`; the prompt's own turn-mates cannot resolve it. |
| **H9** silent abort | Double-Esc before first token: no `Stop`, no marker, no entry; chip stranded green. | Both event streams silent. | Pane-title idle-glyph demotion with a fresh-sample + grace gate (`fc33c1d`). **Dead on the Claude graph path since `784e3a8`**; never ported. | A third signal is required; the rewrite should port H9 into the graph path rather than inherit the gap. |
| **T4** back-dated anchor | Idle-title demotion fired every tick (`age=13m` on a 1 s-old chip). | A teammate's hook re-anchored `StatusSince` to a quiescent main transcript. | Floor the anchor at the previous `StatusSince` (`6105595`). | Anchors never move backward. |
| **L1–L3** orphaned death watch | Three 4½-hour ghost lanes (2026-07-22) after SIGKILL restarts. | pidfd watch lives in daemon memory; restart orphans it; `dropStaleSessions` deleted silently; the reconciler deferred to a callback that never came. | `endSession` single writer; `sweepDeadSessions` every tick; startup stale-drop records `session_end` (`a5c554c`). | Poll-based liveness backstop every tick; record end *before* delete. |
| **L4/L4b/L4c** | — | — | `sessionDead` = `ErrGone` or recycled-to-non-agent only; `ErrUnsupported` is never death. | Three-valued liveness; darwin stub must not end sessions. |
| **L5** double end | Sweep and pidfd race. | — | Map membership is the dedup. | Exactly one `session_end`. |
| **L7** window-closed bare delete | A session removed on `closewindow` never got an end and was invisible to the sweep. | Fourth removal path bypassed `endSession`. | Route through `endSession` only when the pid is definitively gone (`57e46ee`). | Every removal path goes through one end function; a closed window is not process death. |
| **L8** end lost after observation | pid 599441 had one event ever. | `Sink.Record` drops on a full 512 buffer; torn lines are skipped. | Reader-side suspect flag with calibrated caps (`b0bf3cb`). | Flag, never drop; totals held to `suspect_since`. |
| **F3** lane grouping | Six pids hosting four sessions merged; four resumed sessions split. | Reader grouped by pid. | Group by `session_id`, adopt the pid lead-in, exclude global focus/activity streams from routing (`731cb7f`). | Grouping key is session id; focus events never route lanes. |
| **Fanout scroll-out** | Fanned-out agents never appeared; long agents never closed. | Fixed 128 KiB tail parse; inflight recomputed from the window. | `subagents/` dir scan + forward byte cursor + `agent_id`-keyed seen set seeded from history (`8b3bcf8`–`e64a64c`). | Spawn set = meta ∪ jsonl files keyed by `agent_id`; durable inflight; restart seeding. |
| **Launch ack** | Four live agents rendered idle for four hours (2026-08-14). | Parent `tool_result` for `Agent` is a launch ack; a `run_in_background` guard keyed on a field that is never set. | Classify the ack, mark the spawn async, complete only from the child jsonl (`198cc8d`). | Never treat an `Agent`/`Task` `tool_result` as completion; check field distributions before trusting a guard. |
| **Subagent-permission oscillation** (2026-08-05) | Red lost after 8 s; orange/green flap at 5 s for 95 s while a teammate's Bash approval waited. | D1 tool-name match; D2 teammate hooks repaint the parent; D3 back-dated anchor; D4 resolution checked against the wrong transcript; D5 hold gate covered only `PostToolUse`. | Per-writer `Pending` map, `agent_id` + input hash correlators, hold on every hook event, writer-routed resolution, per-writer liveness cap, `pending_writers` persisted (`#58`). | All five: writer ownership, writer-only release, anchors floored, hold generalized, no clear from a non-owner. |
| **M1** Codex question ownership | Codex `AskUserQuestion` red erased by any writer's next hook. | Pending opened only for `request_user_input` on `PreToolUse`. | Open on either onset edge for both tool names; asymmetric id/composite matcher (`f6df139`). | Every published red has an owned record that re-asserts it. |
| **M2** parallel prompts | Answering one of two gated calls turned the chip green. | One prompt per writer; overwrite on open. | `map[writer][]PendingPrompt`, red leaves at `len == 0`, persisted as `pending_prompts` (`c24243b`, `845456e`). | 7.6% of tool turns dispatch ≥2 calls; a writer owns a *set*. |
| **R1/R2/R5** latch races | Id bound to a sibling's call; a two-read rule fooled by burst reads. | First read after the hook precedes the prompt's own flush by ~5 s; one message's parallel entries land 0.5–1.5 s apart. | `callLatchSkewGrace` 3 s, `callLatchConfirmGrace` 2 s by *time* not read count (`1b89a0c`). | Candidate dating and a time-separated second view. |
| **Codex approval red erased** | Published approval red overwritten by the next `Stop`/`PostToolUse`. | No re-assertion path for approvals. | `overlayCodexApprovalObservation` (`8a915e5`). | Records, not transitions, are the authority for red. |
| **Auto-review false red** | 53 red episodes, median 3.8 s, on Auto-review sessions. | 500 ms timeout classified unknown ownership as human. | 30 s grace; user-input immediate (`69d1751`, `bc36f43`). | Elapsed time is not ownership evidence (Phase 2 still open). |
| **Shared-daemon attribution** (2026-08-21) | New Codex TUIs stayed grey; old one worked. | `codex app-server daemon` owned new threads; hook ancestry reached no visible TUI. | Fail closed; never bind by cwd; use disposable stdio app-server (`c3dfe25`–`9af3e30`). | A hook with no tracked ancestor is dropped, and that drop is counted. |
| **A2** nested `claude -p` race | Parent took the helper's session id (35/102 ids on two pids; one session cycled 41 ids). | Ancestor walk skipped an *untracked* agent process to the tracked parent. | Stop at the nearest agent process; drop if untracked (`e6b24ba`). | Ancestor walk stops at the first agent-shaped process, tracked or not. |
| **Address `0x` prefix** | socket2 events omit `0x`; `hyprctl clients` includes it. | — | WM seam owns normalization (`234776a`). | Seam-boundary normalization; conformance round-trip test. |
| **Spinner in the join key** | Terminal and WM read the title at different instants; initial mapping a coin flip. | Spinner glyph inside the compare. | `panetitle.Normalize` at construction (`ea06c36`). | Idempotent normalizer applied at the boundary. |
| **Seed-replay OOM** (2026-08-26) | Daemon killed at 2.5–4 GB in a restart loop. | Two full-store `ReadRange` materializations per session at first sight; store 100× fatter from memory samples. | Streaming shared-pass seed, seed cursor, `MemoryMax=2G` insurance, feature deletion (`e6397e3`, `8d10938`, `082da39`). | Seed cost O(tail) on restart; never materialize the history store. |
| **Publish storm** | 6.9–11.5 publishes/s, 1.4 MB/s JSON, 11 subscribers, for a rotating spinner. | Title in change key; `observed_at`/`fresh_until` churn; hook overlay relabelling the whole Codex graph with a 600 s horizon (provenance flap); ungated federation View. | #4.5 provenance fix, #5–#8.5 (`54b077c`–`0e531b8`), ceiling `fresh_until`. | Change key excludes clocks; quantize at construction; one encode per publish. |
| **Spurious focus events** (#91, #92) | 84 sub-second A→B→A focus alternations in one day. | Two panes in one window both `Focused`; two independent map ranges disagree. **Still open.** | — | Define "the focused session" for a shared window (pane-level `is_active`). |
| **Cross-process clock** | Hook `ObservedAt` stamped in ctl, app-server's in the daemon; `shouldApplyObservation` orders them. **Still open.** | — | — | Stamp observation times in one process. |
| **Remote hysteresis** (PR #90) | Held rows flicker fresh→stale→fresh; a race can wedge a host until restart. | Hold machine derived phase from `lostAt` alone; epoch not bumped on the drop path. | Reverted; one bounded reconnect confirmation instead. | Do not reintroduce a stale-row TTL state machine without the review's findings. |
| **Hyprland Lua dialect** | Focus failed on Hyprland 0.56+. | `keyword`/`dispatch` refused under the Lua parser. | Speak only Lua (`e265eca`); no `;` inside a batch. | WM backend must handle the compositor's config dialect. |

---

## 4. The public contracts

| Contract | Where specified | Consumers | Promised stability |
|---|---|---|---|
| **`state.json`** (`~/.cache/switchboard/state.json`) | `docs/state-schema.md`; golden `internal/state/testdata/state.golden.json` pinned by two tests (round-trip + canonical-snapshot coverage). | waybar, polybar, eww/i3blocks recipes, `claude-tui`, `switchboard-ctl`, user scripts, `remotestate` (federation frames carry whole snapshots). | "Frozen public contract". Stable fields: `pid`, `cwd`, `tty`, `started_at`, `focused`, the `claude` block. Provisional blocks: `wezterm`, `hyprland`. Additive-only evolution; `schema_version` (currently 3) governs *persisted* hydration, not consumers. Atomic temp+rename, two-space indent, trailing newline. Written only on observable change; `updated_at`, `observed_at`, node `updated_at` advisory. |
| **History JSONL** (`$XDG_STATE_HOME/switchboard/history/YYYY-MM-DD.jsonl`) | `docs/history-schema.md`; `internal/history/history.go`. | `switchboard-ctl history/timeline`, the seed index, `switchboard-dashboard` (indirectly). | Additive; only `ts` and `type` guaranteed. Event types: `session_start/end`, `transition`, `suspend/resume`, `subagent_spawn/stop`, `workflow_start/stop`, `agent_state`, `usage_sample`, `session_label`, `focus`, `activity`; `memory_sample` retired but tolerated. Group by `session_id`, fall back to pid. Minimal tier scrubs `cwd`, `pending`, `reason`, `description`, `label`, `nickname`, `role`. |
| **`timeline --json` envelope** | `docs/history-schema.md` §"--json contract (v2)"; types mirrored in `switchboard-dashboard/internal/timeline/types.go`. | `switchboard-dashboard` via `providers.json` exec (process boundary: argv, `--dir/--day/--since/--until`, exit 0 + stdout JSON, `provider_errors`). | Explicitly **out of scope for the clean break** (FR-O9). Durations are nanoseconds; `suspect_*` fields additive; `agent_timeline` omitted when empty. |
| **RPC socket** (`$XDG_RUNTIME_DIR/switchboard.sock`, newline JSON) | `internal/rpc/rpc.go` header comment only; no doc. | ctl, bars, TUI, `remote-stream`, WezTerm Lua. | Commands: `list`, `list-all`, `focus`, `focus-session` (exact `host:pid:started_at` key), `subscribe`, `subscribe-all`, `pane-bind`, `pane-state`, `announce-bindings`, `hook`, `agent-diagnostics`, `activity`. Not promised stable; `subscribe` is host-local so two federating clients cannot recurse. |
| **Hook interfaces** | `README.md`; `docs/claude-code-hook-schema.md` (binary-verified 2.1.222); Codex per OpenAI docs. | Claude Code and Codex hook runners → `switchboard-ctl hook <Event>` / `codex-hook <Event>`. | Fire-and-forget, 2 s timeout, exit 0 always, no JSON output. Forwarded: `session_id`, `transcript_path`, `cwd`, `tool_name`, `tool_use_id`, hashed `tool_input`, `agent_id`, `agent_type`, `observed_at`, plus Codex `turn_id`, and only for Codex naming a 1,000-char prompt / final message. Raw tool input never crosses. Claude `PreToolUse` **must** stay matcher-limited. |
| **Codex `remote-stream` frames** | `docs/ssh-federation-plan.md` §4.1. | The client daemon's SSH worker. | `{"host":…,"snapshot":{…}}` full replacement per line; compatible ctl/daemon revisions required. |
| **OSC binding payload** | `docs/ssh-federation-plan.md` §5.1; `internal/panebind`. | WezTerm Lua `user-var-changed`. | `OSC 1337 SetUserVar=SWITCHBOARD_SESSION=<base64 {host,pid,started_at}>`; bounded, versioned v1. |
| **Telemetry lines** | `docs/telemetry.md`. | Operators, issue reports. | `publish-stats`, `fanout-seed`: fixed field order pinned by test. |
| **Config files** | `history.json` (`enabled`, `detail`, `retain_days`, `max_bytes`), `projects.json` (abbreviations), `~/.config/waybar/claude.jsonc`, `polybar/switchboard.ini`, `hosts/<host>/systemd/*.conf` drop-ins. | — | Unknown keys ignored. |

---

## 5. Requirements for the rewrite, and how far the code has moved past them

### 5.1 What the two docs say

`docs/rust-rewrite-kickoff.md` (2026-08-12): a brief for a *scaffolding* agent.
The owner hand-rolls the tokio spine, canonical types, `/proc` probe, Hyprland
IPC, hook ingest and reducer; the agent builds trace capture from the running
Go daemon (the behavioral oracle, "time-limited"), a replay harness, fixtures,
a v2 schema draft, a hostile fake adapter, and resolves the "fork point"
question (whether `internal/writerstate` lands first). It bounds every
delegated task to ≤6 files and a tool budget because unbounded research agents
died 8/8.

`docs/rust-rewrite-requirements-v0.2.md`: locks D1 clean wire break (v2
schema, renderers ported), D2 Windows in v1, D3 Claude + Codex + open agent
set, D4 per-vendor parsers behind one `SessionLog` trait of neutral questions,
D5 prototype the adapter trait against a hostile third agent. Adds AP7–AP11,
four seams (OS, terminal, WM, UI), the tty-anchored join generalized to a
`SessionAnchor`, FR-G3 "port the writer-state FSM not the 19-rule table",
FR-J terminal seam, FR-K control surface, FR-L lifecycle, FR-M multi-writer,
FR-N history, FR-O v2 wire (incl. encoder configuration pinned byte-level),
FR-P supervision, FR-Q conformance, FR-R clock types, FR-S Codex identity and
naming (08-24 amendment), milestones M-1..M10, and a risk register mirroring §3.

### 5.2 Where `main` has moved beyond both docs

1. **Rule count and shape.** They cite "19 rules in `knobs.go`"; there are now
   **38** ids, half of them graph-path rules (`writer_call_match_cleared`,
   `writer_call_mismatch_held`, `non_owner_prompt_held`, `child_terminal`, …).
   The legacy 18 + `agent_graph_authority` still exist but the legacy
   reconciler is gated off for Claude.
2. **`internal/writerstate` never landed.** FR-G3 tells the port to target that
   FSM; it exists only on branch `docs/writer-state-model` at `2040a91`
   (08-05). The shipped Claude model is instead `internal/provider/claude`'s
   `rootState` with `map[writer][]PendingPrompt`, latch states
   (`CallLatchProposed/Bound/Ambiguous/Contested`), per-writer resolution
   anchors, `Residual` records and `PreToolUse` candidates. FR-G3's "delete the
   subagent dimension" thesis is untested against M2/R1–R5.
3. **Prompt persistence went further than FR-L6.** `pending_prompts` now
   carries `attention`, `since` (prompt onset, distinct from the restart-stamped
   resolution anchor) and a confirmed `call_id`; hydrate falsifies per call.
4. **`PreToolUse` is a registered Claude hook** (matcher-limited), contradicting
   `subagent-permission-plan.md` R2 and the requirements' implicit hook set.
5. **Memory readings are gone.** FR-O1 says v2 must carry "the memory readings"
   and FR-O1c/NFR-1 discuss `mem_agent_bytes`; the feature, the events, the
   ctl surface and the 2 GB default were deleted on 08-26 after the OOM.
6. **Codex model rewritten twice after the docs.** FR-S (08-24) is current on
   identity and naming but predates: the 30 s auto-review grace and
   timeout-to-human Phase 1; the writer-blind Codex sweeps (D-35); M1's
   either-edge opening; the approval re-assertion overlay; exact child-hook
   fusion queues (256 edges, 10 min expiry); the `-codex-observer auto|off`
   flag; and the empty `cmd/switchboard-codex` leftover.
7. **Kickoff task #5 (pane scraping for Codex approvals) is superseded** by
   FR-S4/S6 (marked in the doc), and FR-E8c "probe the rendered pane" should be
   read as rejected.
8. **Federation exists** (`24bc51a`): `-remote` flag, `remotestate`,
   `federation.View`, `panebind`, `list-all`/`subscribe-all`, `focus-session`
   exact keys, `local_workspace`, `resolved_name`, the WezTerm Lua module. The
   requirements never mention multi-host; FR-K1's command list is short by six.
9. **Publish suppression and freshness ceiling** (`SnapshotChangeKey`,
   `CeilFreshUntil`, `AgentGraph.MarshalJSON`, renderer-owned clocks, coalesced
   subscriber wakeups, persistence off the fanout) — none in the docs; FR-O3
   ("written only on change") understates what is now a soundness argument.
10. **Renderer topology changed.** Ten `switchboard-waybar --slot N` processes
    became one `bottombar watch` broker writing atomic slot files and sending
    pidfd-validated `SIGRTMIN+N`; a one-process attached publisher profile
    exists (`switchboard-waybar-publisher.service`, marker
    `.switchboard-combined-v1`). FR-O7 "port the waybar module" now means
    porting `internal/waybarchip` + the broker, not a per-slot binary.
11. **Polybar renderer and i3 X11-PID hydration exist**; `internal/barview`
    is the neutral chip model both bars consume.
12. **Deploy model**: `scripts/deploy`, `~/.local/share/switchboard/releases/<rev>`,
    `current` symlink, `~/.local/bin` links, `internal/buildinfo -version`. The
    docs assume `go install`.
13. **History grew**: `workflow_start/stop`, `agent_state`, seed cursor
    (`.seed-cursor-v1.json`), `history calibrate`, carried names across days,
    activity carry-in and hold cap, launch-ack repair script.
14. **Attention navigation is a bounded ring** (08-31); FR-K2 describes the
    old `attention` semantics.
15. **Diagnostics**: `switchboard-ctl diagnose --observer`, `agent-diagnostics`
    counters (`pretooluse_*`, `prompt_call_*`, `wait_episode`,
    `hook_client_identity_*`), and the restored decision line on
    `applyObservationWithRule` — FR-K5 only asks for the legacy `diagnose`.
16. **Hyprland Lua dialect** (`e265eca`): the WM backend speaks `hl.dsp.focus`
    and `eval hl.config`, not `dispatch focuswindow`.
17. **Headless sessions** (`headless: true`, skipped by navigation) and
    `suspended` exist; the docs list them (FR-O1) but the kickoff's H/L trace
    coverage list does not include the A2 nested-`claude -p` race.
18. **The trace-capture oracle (M-1, task #2) was never built.** No `traces/`
    corpus exists in this repo or in the Go one; the "unobtainable once the Go
    daemon stops" window is still open because the Go daemon still runs.
19. **Kickoff operating constraints** (agents die on >6 files) are an
    observation about one day's tooling, not a project requirement.
20. **LOC**: "~39k" is now 36k non-test + 40k test.

---

## 6. Open questions and planned-but-unbuilt work

**Left unresolved by the docs**

- Pane-level focus: with two panes in one window, which session is *the*
  focused one? (`memory-footprint-plan.md` §1.4.4; issues #91/#92; R2 in
  `attention-latency-report.md`.) `is_active` is parsed from wezterm but never
  carried onto the wire.
- Cross-process observation clocks (`shouldApplyObservation` compares a ctl
  wall clock with a daemon wall clock).
- Codex Phase 2: remove timeout-to-human; what does unknown ownership render
  as (`review_pending`, green, or grey)?
- Case 7 (Esc with teammates in flight) is documented as a judgment call
  resolved to green; not measured.
- `dropStaleSessions` dates a downtime death at restart `now`; the doc offers
  "last known activity" as the alternative and asks that one policy be pinned.
- Whether the `writerstate` FSM's "no `delegating` state" survives the M2
  parallel-prompt and R1–R5 latch findings.
- `HyprlandInfo.Monitor` (decision #6): populate or remove.
- The mixed-downtime residual for *unbound* prompts: a call answered while the
  daemon was down cannot be falsified without an id.
- Ambiguity is "terminal until the next restart" because latch state is not
  persisted.
- `Snapshot` shares pointer fields (decision #5): deep-copy or value types.
- Where does the 4 h / 2 h suspect cap go when the corpus changes? `calibrate`
  is advisory only.
- Duplicate hostnames across federated hosts (explicitly punted).
- Why `nlessfun` has no attachable Codex app-server.

**Planned, not built**

- Phase 4 macOS (`docs/phase4/`): cgo libproc enumerate, kqueue death watch,
  darwin CI job, AeroSpace WM; plus `discovery` → `osproc.Enumerate`
  repointing and Darwin-aware `IsClaude`.
- Windows (D2 of the requirements): session anchor spike (OQ11) never run.
- tmux → client tty → WM window raise (portability 3.3); kitty backend (3.2).
- Verified eww/i3blocks recipes (5.3).
- H9 idle-title recovery on the graph path (R7).
- Adaptive `Observe` tick while any root is red (Q4/R6).
- Memory-footprint #9 (title-only WM event rate limit), Phase 3 (lazy Codex
  app-server spawn with `IdleShutdown`), Phase 4 GC headroom, Phase 5 results.
- Streaming `timeline` fold (ctl still materializes the full range; a ctl was
  OOM-killed on 08-06).
- Per-span blocked intervals so a repaired subagent span does not credit a
  permission wait as delegated work.
- Workflow agent token usage and per-agent labels.
- Passing the live pid set into `BuildSwimlanes`.
- Persisting `is_active` / pane-accurate `focused`.
- Live acceptance for: SSH federation §12 checks, SSHMUX, PreToolUse Phase D
  counters, Codex #83 rollout capture, `/clear`-then-implement-plan replay
  (#86), i3 live DoD, sway/X11 live DoD.
- `codex-session-status` evidence report questions 1–9 (notification ordering,
  completion retention, proxy attach semantics) are still "unresolved".

---

## 7. What the author would likely drop

Judged from the docs' own retrospectives, the DONE log (abandoned after Phase
0), the incident cost, and what shipped versus what was verified.

1. **Per-session memory sampling.** Built 08-03 with a measured lock budget and
   a live verification run, then deleted 08-26 after it fattened the history
   store 100× and OOM-killed the daemon in a restart loop. The docs call it
   "scope creep" in their own words. Do not port; the requirements doc's
   FR-O1/FR-O1c/NFR-1 references to it are stale.
2. **Remote hysteresis / stale-row hold machine.** Five commits, a 15-finding
   adversarial review, a full revert the next day. The replacement is one
   bounded reconnect. Do not port the hold machine.
3. **The Codex wrapper, "slot sessions", per-TUI endpoints and shared-daemon
   attachment.** Tried 08-21 (`e08fd21`), retracted by the interview
   retrospective as "a scope failure, not a missing setup note". FR-S records
   the correct shape; the planning packet's proxy assumptions are history.
4. **The legacy Claude reconciler layer** (`selfHealStaleAttention`,
   `selfHealStuckStatus`, `rpc.handleHook`'s status mapping, 18 `case*` rules,
   most `Tuning` knobs). Dormant for Claude since 08-21; its one live behavior
   worth keeping (H9) is exactly the one that was lost. Port the graph-path
   model and re-add H9 there; do not port two status engines.
5. **The ten-process Waybar slot architecture.** Replaced by one broker; the
   requirements' "port the waybar module" should mean the broker.
6. **`switchboard-ctl memory`, `seed-bench`, `sb-mem-baseline`, `sb-perf`,
   `scrub-memory-samples`, `repair-launch-ack-spans`.** Measurement and repair
   tooling for incidents that a rewrite should design out; keep the *telemetry
   lines*, not the scripts.
7. **Token cost pricing table** (`internal/history/pricing.go`). Hard-coded
   per-model dollar rates that "are wrong the moment prices change"; the doc
   itself calls plan-window consumption the more useful metric.
8. **Hook-only Codex `hook_client_identity_*` probe counters and the
   `agent-diagnostics` firehose.** Spike instrumentation that outlived its
   probe; keep finite categories, drop the spike-specific ones.
9. **The 2 GB history default and `history calibrate`'s day-file replay.**
   Both existed only because of the sampling volume.
10. **Kickoff Phase A scaffolding tasks** as written: trace capture from
    journald decision lines is no longer representative (the graph path logged
    `agent_graph_authority` for every transition from 08-21 to 08-31), so the
    corpus would need re-capture with the restored rule ids anyway.

What the docs are clear the author would **not** drop, despite cost: the
suspect post-check, the per-writer prompt set, exact Codex attribution, the
publish-suppression proof, the conformance suites, and the history day-files
(the dashboard's only input).

---

## Appendix — where to look first in the Go tree for each area

| Area | Files |
|---|---|
| Status model, Claude | `internal/provider/claude/observer.go` (Observe/ApplyHook/latch), `projection.go`; `cmd/switchboard/agent_observation.go` (authority gate, overlays, decision line) |
| Status model, Codex | `cmd/switchboard/codex_hook_transitions.go`; `internal/provider/codex/observer.go`, `namer*.go` |
| Neutral graph + reducer | `internal/agentgraph/` (`reduce.go`, `types.go`) |
| Hazard tests | `cmd/switchboard/timing_hazards_test.go`, `session_lifecycle_test.go`, `internal/history/suspect_test.go`, `internal/rpc/writer_match_test.go` |
| Wire | `internal/state/state.go`, `agent_graph.go`, `display_name.go`, `testdata/state.golden.json` |
| History | `internal/history/{history,sink,reader,timeline,suspect,seed,seedcursor,agent_state,carryover}.go` |
| Seams | `internal/osproc`, `internal/terminal`, `internal/wm`, `internal/detect`, `internal/conformance` |
| Fanout | `internal/fanout/observer.go`, `internal/transcript/{subagents,workflows,transcript}.go` |
| Federation | `internal/remotestate`, `internal/federation`, `internal/panebind`, `integrations/wezterm/switchboard.lua` |
| Renderers | `internal/waybarchip`, `internal/barview`, `internal/barlayout`, `cmd/switchboard-polybar`, `cmd/claude-tui` |
| Rules/knobs | `internal/statustune/{knobs,statustune,parse}.go` |
