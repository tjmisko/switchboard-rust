I found 20 bugs you can reproduce, and no panics anywhere. Every bug has a test that fails against golden today: 15 in `adv_bugs.rs`, 5 in `adv_daemon_bugs.rs`. The live fd-exhaustion bugs were also reproduced against the real binaries with the Python and shell scripts. The copy is at `~/.cache/sbr-audit/adversarial`; baseline was 241 passing. No daemon processes are left running.

## Bugs, by severity

**High**

1. **A rate-limit block never lifts.**
   - **What happens:** once an account is marked rejected it stays that way; nothing looks at `resetsAt`. A months-old error line in the 128 KiB replay window blocks every agent on the account. On this machine the status line isn't wired to `switchboard-hook`, so no "allowed" update ever replaces it.
   - **Corpus evidence:** 24 files end Blocked this way; 7 of them kept working normally 1–22 h after the reset.
   - **Root cause:** `ingest/claude/transcript.rs:188-209` (stamps the snapshot with `ctx.now` and ignores the line's timestamp), `reduce/snapshot.rs:30-54`, `model.rs:1186`, `reduce/tick.rs` (no expiry).
   - **Fix:** drop `quotaLimits` whose `resetsAt` is already past, stamp them with the line's own time, and add a reset deadline to `next_deadline` and `tick`.
   - **Tests:** `should_not_block_every_agent_when_a_replayed_rate_limit_line_is_months_old`, `should_clear_blocked_once_the_limit_resets_even_without_a_statusline`.
2. **A nested `claude -p` hijacks the agent that spawned it.**
   - **What happens:** `resolve` walks past the untracked `claude -p` up to the tracked agent. `SessionStarted` skips the `/clear` guard, so the agent's session rotates to the nested one and every later hook from the real session is dropped.
   - **Real-world basis:** the corpus has 17 Bash calls running `claude -p`, and the session-summary script's comments confirm print mode fires hooks.
   - **Root cause:** `daemon/rpc.rs:143-161`, `reduce/event.rs:47-49, 84-100`.
   - **Fix:** stop the ancestor walk at the first process named `claude`, and return None if that one isn't tracked. Only rotate the session on `clear`/`resume`.
   - **Test:** `should_not_let_a_nested_print_mode_session_hijack_its_ancestor_agent`.
3. **One failed sweep marks every agent dead.**
   - **What happens:** if `/proc` can't be read, the sweep returns nothing, and every missing process becomes `Died`. Live, under fd exhaustion, both agents were forgotten and re-found 10 s later; the `session_end` records failed to write.
   - **Root cause:** `proc.rs:173-185` (also `:146` for a single pid), `daemon/scanner.rs:54-67`.
   - **Fix:** make `sweep` return a `Result` and skip the death check when it fails; for a single unreadable pid, only declare death on `NotFound`.
   - **Test:** `should_not_declare_every_agent_dead_when_proc_cannot_be_read`.
4. **Running out of file descriptors pins a core and stalls the agents' hooks.**
   - **What happens:** the accept loop retries immediately on EMFILE at 103% CPU and stops answering `list`. The connection backlog then fills (4097), and `switchboard-hook` blocks in `connect()` until killed (10 s in the test).
   - **Why it's reachable:** a systemd user unit's default soft file limit is 1024, and idle connections are never timed out (bug 18).
   - **Root cause:** `daemon/rpc.rs:40-42`; `client.rs:21` (connect has no timeout).
   - **Fix:** back off about 100 ms on accept errors. In the forwarder, use a non-blocking connect with the 500 ms budget.
   - **Repro:** `should_return_within_the_hook_budget_when_the_daemon_is_not_accepting`, plus `adv_daemon_fuzz.py emfile` and `adv_cascade.py 4100`.

**Medium**

5. **Escape before any assistant output is missed** (medium-high).
   - **What happens:** 24 of 51 "prompt then `[Request interrupted by user]`" lines have no `interruptedMessageId`, and no hook fires. The chip stays green Working for 10 minutes, then goes Idle, never Stopped. 11 corpus files end stuck like this.
   - **Root cause:** `transcript.rs:87-97, 115-134`.
   - **Fix:** also treat a text block starting with `[Request interrupted by user` as an interrupt (both marker strings share that prefix).
   - **Test:** `should_stop_the_turn_when_escape_is_pressed_before_any_assistant_output`.
6. **Session drift never self-repairs.**
   - **What happens:** if the `SessionStart{clear}` hook is lost, the agent keeps the old session forever and all new hooks are dropped.
   - **Root cause:** `snapshot.rs:164-178` only fills a session that is None.
   - **Fix:** adopt the sweep's session when it differs; the `procStart` match already proves it's the same process.
   - **Caveat:** I didn't verify that `sessions/<pid>.json` updates on `/clear`.
   - **Test:** `should_adopt_the_session_the_sweep_reports_when_the_clear_hook_was_lost`.
7. **A subagent's permission prompt is lifted by any other call of that subagent settling.**
   - **Why it bites here:** `SubagentStart` isn't registered on this machine but `PostToolUse` is unfiltered, so a parallel Read finishing clears a pending Bash prompt.
   - **Root cause:** `event.rs:145-149, 490-501`.
   - **Fix:** only lift on a settle whose hash or tool matches.
   - **Test:** `should_keep_a_subagent_prompt_while_a_parallel_call_of_that_subagent_settles`.
8. **Image prompts start no turn** (medium-low).
   - **What happens:** 60 corpus prompts are `[text, image]`. After an interrupt, a successful image follow-up leaves the chip amber Stopped.
   - **Root cause:** `transcript.rs:102`.
   - **Fix:** treat array content with a text block and no `tool_result` as a prompt.
   - **Test:** `should_start_a_turn_when_a_human_prompt_carries_an_image`.
9. **Replay writes stale history** (low-medium). Each replayed turn is settled separately, so every rediscovery appends a burst of transitions stamped "now"; I saw six in one second live.
   - **Root cause:** `daemon/watcher.rs:77-87` with the per-event settle at `event.rs:68`.
   - **Fix:** suppress history for the replay batch and settle once at the end.
   - **Test:** `should_not_append_history_for_each_turn_of_a_replayed_transcript`.

**Low** (these come from the property run and edge cases)

10. **A duplicate settle lifts another call's parked prompt.** Root cause `event.rs:351-365`: `lift_sole_unjoined` runs when the call isn't found. Test: `should_keep_a_parked_prompt_when_a_duplicate_settle_arrives_for_another_call`.
11. **A duplicate dispatch consumes a parked prompt.** Root cause `event.rs:325-337`: it takes the prompt before checking whether the call already exists. Fix: check first. Test: `should_not_consume_a_parked_prompt_on_a_duplicate_dispatch`.
12. **A repeated TurnStarted clears parked prompts.** Root cause `event.rs:111-118`: the clear runs before the duplicate check. Test: `should_keep_parked_prompts_when_the_same_turn_start_is_repeated`.
13. **Duplicate SessionStarted and CompactionStarted aren't no-ops.** They always publish, contrary to the promise at `event.rs:3-6`; see `event.rs:80-105, 220-226`. Test: `should_publish_nothing_for_a_duplicate_session_start_or_compaction_start`.
14. **An agent discovered under a rejected limit is never re-evaluated.** It shows Unknown while its true phase is Blocked, and no deadline ever fixes it (`snapshot.rs:180-192`). Test: `should_show_blocked_for_an_agent_discovered_while_the_account_is_rejected`.
15. **Any confirmed hook skips a pending 10 s wait.** An unrelated hook (e.g. SessionEnd) passes its own confidence to settle and promotes an inferred Blocked immediately (`event.rs:68`). Test: `should_not_skip_an_inferred_dwell_because_an_unrelated_hook_is_confirmed`.
16. **Compaction expiry causes one extra core wakeup.** `tick.rs:35-61` checks for a stale turn before expiring compaction, so a second tick is needed. It never loops forever. Test: `should_leave_no_past_deadline_after_a_single_tick`.
17. **A death can remove the wrong agent.** Died for an untracked pid falls back to session routing and removes an agent whose session is `""` (`reduce/mod.rs:66-76`, `scanner.rs:78`). Test: `should_not_remove_an_agent_when_an_untracked_process_dies`.
18. **Connections that never finish a line are held forever.** 5000 were held live (fds 12 → 5012, RSS 55 MB); `rpc.rs:68-70` has no read timeout. Test: `should_drop_a_connection_that_never_completes_a_line`.
19. **A non-UTF-8 line closes the connection with no reply.** 488 of 500 random-byte lines got no reply (`rpc.rs:70`). Test: `should_reply_with_an_error_and_keep_serving_after_a_non_utf8_line`.
20. **Nits:** `Tail`'s `max_line` counts the newline, so it is one byte stricter than "longer than this". The history reader reports 14 errors for Go `session_start` lines that have no `cwd`.

## Stats from the real data

| Surface | Result |
|---|---|
| Transcript corpus | 367 files, 400 MiB, 112,735 lines; 0 panics, 0 parse errors; 5.26 ms/MiB (worst file 9.15) |
| Adapter output | 1,016 human TurnStarted, matching string prompts exactly; 19,142 dispatched vs 19,104 settled; 0 settles without a dispatch; 0 tools open at EOF |
| Oversized lines | 42 lines over 1 MiB skipped (37 tool results, 4 image prompts) |
| Final phases | 290 Idle, 33 Working (Idle after a stale tick), 28 Blocked (24 from limits), 11 Unknown (no events), 5 Stopped |
| 128 KiB replay window vs full replay | 8 files end in a different phase: 7 due to bug 1, 1 grey mid-turn |
| History | 69 files, 50.6 MiB, 7,131 records, 129k Go lines skipped, 14 errors, 0 panics, 6.96 ms/MiB |
| Live `/proc` sweep | Found exactly the 2 interactive `claude` processes, no false positives or negatives; 5 ms per sweep; the test's own ancestor chain resolves to pid 6002 |

## What held up
- **Reducer invariants:** 10,000 random seeds × 300 steps with no panic, no Attention without a human gate, and none ever from Unknown-owner gates. At most one publish, always last. The deadline always clears, and parked prompts stayed bounded (max 4).
- **Socket:** malformed JSON, recursion bombs, pids out of range, a 2 MiB unterminated line and a 900 KiB valid frame were all handled. 3000 concurrent hooks all got replies in 0.06 s. 300 non-reading subscribers through 225k view changes left RSS flat, and a reading subscriber still got updates.
- **Shutdown:** SIGTERM exits in about 3 ms and removes the socket, even with blocked clients.
- **Forwarder:** exits 0 in under 15 ms with garbage stdin, empty stdin, 8 MiB stdin, the daemon down, or the socket path being a regular file. It gives up in 510 ms when the server never replies; the only hang is bug 4.
- **Tail:** 300k lines through split writes, CRLF, and exact skipping of oversized lines, across three runs, with no loss, duplication or reordering.

## Test files (all in `~/.cache/sbr-audit/adversarial/tests/`)
- **Failing repros:** `adv_bugs.rs`, `adv_daemon_bugs.rs`
- **Corpus and data harnesses** (`#[ignore]`, run with `--release -- --ignored`): `adv_corpus.rs`, `adv_history.rs`, `adv_proc.rs`, `adv_props.rs`
- **Tail (passing):** `adv_tail.rs`
- **Live daemon and forwarder:** `adv_daemon_fuzz.py`, `adv_cascade.py`, `adv_forwarder.sh`
- **Corpus shape analyses** (structure only, no content): `adv_shape_interrupts.py`, `adv_shape_array_prompts.py`, `adv_shape_prompt_tails.py`, `adv_shape_nested_claude.py`

The audit daemon's state (24 KB) is in `~/.cache/sbr-audit/state`.