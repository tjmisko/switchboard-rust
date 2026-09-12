I found five places where the golden itself behaves differently from the spec or its own docs. I also found about 30 real coverage gaps. The baseline reproduced 241 passing tests. I ran 96 hand mutations and 53 survived.

Nothing under `~/Projects/switchboard-rust/.worktrees/golden` was touched. All work was done in `~/.cache/sbr-audit/coverage`, which is back to clean and still passes 241. Two things to know about that directory:
- **Other agents are writing there too:** `adversarial/`, `chrome-prof*` and `shape*.py` are not mine.
- **Files I left behind:** a local `git init`, used to revert each mutation; the driver `~/.cache/sbr-audit/mutate.py`; and raw results in `~/.cache/sbr-audit/mutations.jsonl`.

## A. Behaviour the golden gets wrong

I confirmed each of these with a throwaway test in the copy, which failed and was then deleted. A new test for any of them would fail today, so each needs a decision or a fix before a test can pin it.

| # | Where | What happens |
|---|---|---|
| A1 | `view.rs:150` `human_gate` | Ignores subagent gates. The chip is Attention (red) but the tooltip says "1 subagent(s) running". The doc comment claims the tooltip "follows the same order as `Agent::phase`"; it does not. |
| A2 | `view.rs:108` `reason` | An unowned gate makes the phase Unknown (grey), but the tooltip says "running 1 tool call(s)". |
| A3 | `view.rs:112` | A rejected limit with no `resets_at` makes the phase Blocked, but the tooltip says "idle". |
| A4 | `snapshot.rs:164` `discover` | A `Processes` sweep taken before a `Died` brings the dead agent back. Nothing checks `AsOf` or remembers dead keys. Today only the scanner's send order (sweep, then deaths, from one task) prevents this. |
| A5 | `event.rs:47,84` | A `SessionStarted` for the old session arriving after `/clear` switches the agent back to the old session. The race guard exempts `SessionStarted`. This is unlikely in practice. |

Two open questions on these:
- For A4, is the scanner's send order the intended guarantee, or should the reducer enforce it?
- `SessionStarted` always returns `true` (`event.rs:104`), so a duplicate always publishes and stamps `last_event`. Is that intended?

## B. Prioritised gaps with test proposals

**P0: cost-model rules nothing pins**

1. **R1, user-caused escalation on its own.** Mutation S2 (delete `cause == Cause::User`) survives, because every user-caused test is also `Confirmed`.
   `should_escalate_immediately_when_the_user_caused_it_even_if_evidence_is_inferred`
   ```
   let mut s = Shown::new(Phase::Working, t0);
   assert!(s.settle(Phase::Stopped, Cause::User, Confidence::Inferred, t0));
   // reducer level: a turn, then an inferred interrupt event with cause=User -> shown == Stopped at once
   ```
2. **R3, only a confirmed signal may raise Attention.** S6 (an inferred Attention lands at once), M9 (an unjoined prompt with owner Unknown is ignored) and H4 (Elicitation owner set to Unknown) all survive.
   `should_hold_an_inferred_attention_for_its_dwell_when_the_agent_caused_it`: `settle(Attention, Agent, Inferred)` returns false and sets `pending`.
   `should_render_an_unjoined_unowned_prompt_grey_when_idle`: `PermissionAsked{call: None, owner: Unknown}` gives truth Unknown.
   `should_raise_a_human_question_gate_when_an_elicitation_hook_arrives`: the Elicitation frame gives `PermissionAsked{owner: Human, kind: Question, call: None}`, and the reducer then shows Attention.
3. **`Agent::phase` precedence.** M2 (limits before Attention), M3 (Stopped before limits), M5b (Waiting before Delegating) and M6 (Pending subagents don't delegate) all survive.
   `should_rank_phases_by_precedence_when_several_axes_hold`, table-driven:
   ```
   for (setup, limits, expect) in [
     (gated_turn, Some(rejected), Attention),     (idle_after(Interrupted), Some(rejected), Blocked),
     (idle + subagent Running + wait, None, Delegating), (idle + subagent Pending, None, Delegating),
     (unobserved + human unjoined, None, Attention), (idle_after(Failed) + subagent Running, None, Blocked)]
   { assert_eq!(setup.phase(limits.as_ref()), expect) }
   ```
   Only three of the pairs you asked about are pinned: Attention over Working, Attention over Delegating, and the unowned gate over Working. Attention over Blocked, Blocked over Stopped, Delegating over Waiting, Stopped/Blocked over Delegating, and Unknown (never heard from) over Delegating are all unpinned.
4. **R4, lifting without a call id.** E24 (no call id), E25 (Abandoned), E26 (subagent `PermissionSettled`) and H5 (`PermissionDenied` drops its id) all survive.
   `should_lift_the_sole_parked_prompt_when_a_permission_settles_without_a_call_id`: park `asked(Bash)`, then `PermissionSettled{call: None, Denied}`; truth is not Attention.
   `should_drop_the_call_when_its_permission_is_abandoned`
   `should_lift_a_subagent_gate_when_its_permission_settles`
   `should_carry_tool_use_id_when_adapting_permission_denied`: a fixture-backed adapter test.
5. **Duplicate close from hook plus transcript.** E8 survives. A hook `Stop` followed by the transcript's `end_turn` is the most common duplicate in real use.
   `should_publish_nothing_when_the_transcript_end_turn_follows_a_hook_stop`: `stop()` then `inferred(stop())`; the effects are empty.
   `should_keep_the_first_failure_when_an_interrupt_line_arrives_later` (E9): Failed then Interrupted gives Blocked.

**P1: code no test reaches at all**

6. **Compaction.** T1, T3, T6, E16 and H3 all survive.
   `should_not_end_a_quiet_turn_while_compaction_is_running`: turn, `CompactionStarted`, then a tick at `TURN_STALE+1s` is still Working.
   `should_forget_a_compaction_after_its_stale_limit`: a tick at exactly `since + COMPACTION_STALE` gives `Compaction::None`.
   `should_include_compaction_expiry_in_next_deadline`
   `should_publish_nothing_for_a_second_compaction_end`
   `should_end_compaction_when_post_compact_hook_arrives`
7. **Tick leak bounds.** T2 (unjoined prompts never expire) and T5 (`next_deadline` ignores waits) survive.
   `should_drop_an_unjoined_prompt_after_its_expiry`: park at t1; a tick at `t1 + UNJOINED_EXPIRY` leaves `unjoined` empty and truth not Attention.
   `should_wake_for_a_wait_expiry_when_nothing_else_is_pending`: `next_deadline == Some(expires)`.
8. **Statusline adapter and Beat snapshot, both zero coverage.** L1–L3 and N7 survive.
   `should_scale_a_fractional_utilization_to_percent_when_adapting_a_statusline`: 0.5 → 50, 87 → 87, 1.7 → 2.
   `should_emit_limits_and_a_beat_when_the_statusline_carries_both`
   `should_keep_the_newer_beat_when_an_older_statusline_arrives_late`
9. **Placements and `focused`, both zero coverage.** N5 and N6 survive.
   `should_publish_only_when_a_placement_changes`: apply the same `Placements` twice; the second publishes nothing, and `View.focused` follows `focused: true`.
10. **Panel-owned subagents and the subagent tool arms.** E11, E13, E14 and E20 survive.
    `should_mark_a_panel_row_completed_and_lift_its_gate_when_subagent_stop_arrives`
    `should_join_a_parked_subagent_prompt_when_that_subagent_dispatches_the_call`
11. **The join.** E21 and E22 survive.
    `should_gate_the_second_identical_call_when_a_second_prompt_arrives`: two Bash calls with the same input and two call-less asks leave both gated.
    `should_park_two_prompts_for_one_tool_when_their_hashes_differ`
12. **Hook arms never exercised: SessionEnd, PostCompact, Elicitation, PermissionDenied.** H6 (SessionEnd emits `Died`) survives, which is dangerous. Also unexercised: `source: "system"` (H2), `pending` background tasks (H8), `PermissionRequest` for a question tool (H9), and the `seven_day*` window (R4).
    `should_emit_session_ended_not_died_when_adapting_session_end`
13. **`AsOf` boundaries.** M1 (`>` to `>=`) and N1 survive.
    `should_drop_a_panel_taken_at_the_same_instant_as_the_last_event`

**P2: daemon glue, binary, view text**

14. **Daemon tasks.** `effects::run` (D3), `history::run` (Y3), `publish::run` (D2, coalescing) and the core's identical-view check (D1) all survive. `scanner::run`, `daemon::run`, `bind`'s stale-socket removal, rpc's `Statusline` branch and `offer` on a full inbox have no test.
    `should_abort_the_watcher_when_the_runner_receives_forget`
    `should_append_not_truncate_when_history_task_writes_twice`
    `should_write_state_once_for_a_burst_inside_the_coalesce_window` (paused time, several `view.send`, count writes)
    `should_remove_a_stale_socket_file_when_no_daemon_answers`
15. **`switchboard-hook` binary, zero coverage.** D6 survives.
    `should_forward_a_statusline_frame_and_exit_zero_when_the_daemon_is_down`: run `CARGO_BIN_EXE_switchboard-hook`.
16. **View text.** V1 (`phase` reports truth), V2 and V3 survive; only one `Reason` variant has asserted text.
    `should_describe_every_reason_variant`, table-driven.
    `should_publish_shown_not_truth_while_an_escalation_is_pending`
17. **`read_day` stops on a hard io error.** Y1 survives. Needs a reader that fails partway.
18. **`time.rs` edge cases, all untested.**
    - Leap second `:60` (Z1).
    - Invalid dates are accepted: `2026-02-31` parses as March 3.
    - Offset minutes are not range-checked: `+05:99` is accepted.
    - Dates before 1970 return `None`, and `format_rfc3339` silently prints them as 1970.
    - The lowercase `t`/`z` and space-separator branches.
    - More than 9 fractional digits.

    `should_reject_a_day_past_the_end_of_the_month`, `should_accept_a_leap_second`, `should_return_none_for_a_pre_epoch_timestamp`.

**Functions nothing calls:** `Board::by_session` (it only appears inside a test name), `HistoryWriter::flush` and `get_ref`, `Watchers::is_empty`, `paths::socket_path`, `home`, `Layout::from_env`.

## C. Mutation table

Killed: 43 of 96. Line numbers refer to the golden source.

| id | file:line | mutation | killed | killing test |
|---|---|---|---|---|
| E1 | event.rs:49 | drop /clear guard | y | should_ignore_a_late_event_for_the_previous_session_after_clear |
| E2 | event.rs:85 | clear with same session doesn't reset | n | |
| E3 | event.rs:101 | SessionStart keeps Unobserved | y | t07 ×10; t10_core **hangs** |
| E4 | event.rs:111 | TurnStarted keeps unjoined | y | should_lift_every_agent_prompt_when_the_turn_ends_or_a_new_one_starts |
| E5 | event.rs:116 | keyless restart treated as duplicate | n | |
| E6 | event.rs:282 | end_turn keeps unjoined | y | same as E4 |
| E7 | event.rs:286 | different-prompt end closes turn | y | should_ignore_a_turn_end_that_names_a_different_turn |
| E8 | event.rs:297 | duplicate Stop counts as change | n | |
| E9 | event.rs:302 | late outcome overwrites explained one | n | |
| E10 | event.rs:151 | interrupt flag inverted | y | should_end_the_turn_as_interrupted_when_a_tool_fails_by_interrupt |
| E11 | event.rs:137 | subagent dispatch doesn't join | n | |
| E12 | event.rs:181 | panel guard on SubagentStarted removed | y | should_not_let_events_add_or_remove_rows_when_the_panel_owns_them |
| E13 | event.rs:203 | SubagentEnded removes panel row | n | |
| E14 | event.rs:207 | panel SubagentEnded keeps gate | n | |
| E15 | event.rs:216 | SubagentEnded keeps parked prompts | y | should_park_a_prompt_from_an_unknown_subagent_and_lift_it_when_that_subagent_ends |
| E16 | event.rs:229 | duplicate CompactionEnded publishes | n | |
| E17 | event.rs:273 | hash_admits ignores hash | y | should_not_join_a_prompt_to_a_call_with_a_different_input |
| E18 | event.rs:335 | duplicate dispatch overwrites | y | should_publish_nothing_when_the_same_call_is_dispatched_twice |
| E19 | event.rs:364 | unseen settle lifts nothing | y | should_lift_the_sole_parked_prompt_when_an_unseen_call_settles |
| E20 | event.rs:390 | subagent re-ask reports change | n | |
| E21 | event.rs:422 | call-less ask re-gates a blocked call | n | |
| E22 | event.rs:446 | park dedupes across hashes | n | |
| E23 | event.rs:485 | lift_sole lifts several | y | should_keep_both_parked_prompts_when_an_unseen_call_settles_ambiguously |
| E24 | event.rs:511 | PermissionSettled without call id is a no-op | n | |
| E25 | event.rs:528 | Abandoned keeps call | n | |
| E26 | event.rs:176 | subagent PermissionSettled no-op | n | |
| E27 | event.rs:498 | lift_subagent doesn't clear | y | should_raise_attention_from_a_subagent_gate_and_keep_it_through_the_agents_stop |
| E28 | event.rs:62 | stamp last_event on no-op | y | should_stamp_last_event_only_when_something_changed |
| M1 | model.rs:495 | supersedes `>` to `>=` | n | |
| M2 | model.rs:1182 | limits before Attention | n | |
| M3 | model.rs:1186 | after-outcome before limits | n | |
| M4 | model.rs:1201 | Working before unowned gate | y | should_render_an_unowned_gate_grey_never_red_even_mid_turn |
| M5 | model.rs:1220 | waits don't produce Waiting | y | should_wait_on_background_tasks_reported_by_stop_and_forget_them_when_expired |
| M5b | model.rs:1214 | Waiting before Delegating | n | |
| M6 | model.rs:1218 | Pending doesn't delegate | n | |
| M7 | model.rs:1210 | Unobserved falls to Idle | y | should_show_unknown_for_a_discovered_agent_until_it_is_heard_from |
| M8 | model.rs:1264 | ignore subagent gates | y | should_raise_attention_from_a_subagent_gate…; should_keep_a_gate_an_event_set_on_a_row… |
| M9 | model.rs:1287 | unowned unjoined ignored | n | |
| S1 | model.rs:1089 | promote `>=` to `>` | y | should_promote_a_pending_escalation_on_the_tick_at_its_deadline (+4) |
| S2 | model.rs:1073 | user-caused escalation waits | **n** | |
| S3 | model.rs:1073 | confirmed escalation waits | y | should_escalate_immediately_when_evidence_is_confirmed (+7) |
| S4 | model.rs:1088 | inherit dwell across phase change | y | should_restart_the_dwell_when_the_pending_phase_changes |
| S5 | model.rs:1083 | snapshot uses inferred dwell | y | should_hold_blocked_for_one_snapshot_dwell_when_the_account_is_rejected |
| S6 | model.rs:1087 | inferred Attention immediate | **n** | |
| N1 | snapshot.rs:32 | Limits `<=` to `<` | n | |
| N2 | snapshot.rs:85 | panel ignores staleness | y | should_drop_a_panel_taken_before_the_last_event |
| N3 | snapshot.rs:209 | panel drops gates | y | should_keep_a_gate_an_event_set_on_a_row_the_panel_still_shows |
| N4 | snapshot.rs:231 | wait `armed` reset | y | should_wait_on_background_tasks… |
| N5 | snapshot.rs:127 | Placements publish unchanged | n | |
| N6 | snapshot.rs:130 | Placements never stored | n | |
| N7 | snapshot.rs:64 | older Beat overwrites | n | |
| N8 | snapshot.rs:174 | sweep session adoption no publish | n | |
| N9 | snapshot.rs:104 | Waits routed to wrong agent | y | should_wait_on_background_tasks… |
| T1 | tick.rs:57 | compaction expiry `>` | n | |
| T2 | tick.rs:53 | unjoined never expire | n | |
| T3 | tick.rs:78 | stale turn ignores compaction | n | |
| T4 | tick.rs:46 | wait expiry `>=` | y | should_wait_on_background_tasks… |
| T5 | tick.rs:105 | deadline ignores waits | n | |
| T6 | tick.rs:104 | deadline ignores compaction | n | |
| T7 | tick.rs:74 | stale ends gated turn | y | should_never_end_a_turn_that_is_parked_on_a_gate |
| H1 | hook.rs:171 | interrupt failure gets Cause::Agent | y | should_attribute_an_interrupted_tool_failure_to_the_user |
| H2 | hook.rs:299 | `system` becomes Machine | n | |
| H3 | hook.rs:247 | PostCompact dropped | n | |
| H4 | hook.rs:274 | Elicitation owner Unknown | n | |
| H5 | hook.rs:138 | PermissionDenied drops id | n | |
| H6 | hook.rs:262 | SessionEnd emits Died | **n** | |
| H7 | hook.rs:70 | clear gets Cause::Agent | y | should_start_a_session_caused_by_the_user_when_source_is_clear |
| H8 | hook.rs:330 | `pending` tasks dropped | n | |
| H9 | hook.rs:121 | question PermissionRequest becomes Approval | n | |
| L1 | statusline.rs:66 | utilization not scaled | n | |
| L2 | statusline.rs:27 | rejected becomes Warning | n | |
| L3 | statusline.rs:61 | beat dropped | n | |
| R1 | transcript.rs:127 | user-rejected gets Cause::Agent | y | should_attribute_interrupts_denials_by_the_user_and_typed_prompts_to_the_user |
| R2 | transcript.rs:103 | local commands start turns | y | should_turn_the_fixture_into_exactly_this_sequence |
| R3 | transcript.rs:57 | sidechain not ignored | y | should_turn_the_fixture_into_exactly_this_sequence |
| R4 | transcript.rs:216 | seven_day unrecognised | n | |
| A1 | tail.rs:79 | truncation keeps carry | y | should_restart_from_zero_and_drop_the_carry_when_the_file_shrinks |
| A2 | tail.rs:110 | oversize tail not skipped | y | should_skip_an_oversized_line_whole_even_when_it_spans_polls |
| A3 | tail.rs:113 | partial line emitted | y | should_hold_a_partial_line_across_several_polls (+3) |
| A4 | tail.rs:82 | truncation not reported | y | should_restart_from_zero_when_the_file_shrinks |
| Y1 | history.rs:216 | continue after hard io error | n | |
| Y2 | history.rs:126 | held_ms not read | y | should_round_trip_every_record_kind_through_a_buffer |
| Y3 | daemon/history.rs:20 | overwrite instead of append | n | |
| V1 | view.rs:80 | JSON phase = truth | n | |
| V2 | view.rs:203 | no "thinking" | n | |
| V3 | view.rs:135 | reason skips delegating | n | |
| D1 | core.rs:83 | publish identical views | n | (the test meant to catch it can never fail, see D) |
| D2 | publish.rs:22 | no coalesce | n | |
| D3 | effects.rs:45 | Forget keeps watcher | n | |
| D4 | daemon/mod.rs:140 | socket mode not 0600 | y | should_serve_a_real_unix_socket_end_to_end |
| D5 | watcher.rs:87 | never Confirmed | y | should_replay_the_tail_as_inferred_then_follow_new_lines_as_confirmed |
| D6 | switchboard-hook.rs:49 | statusline sent as hook | n | |
| D7 | daemon/mod.rs:128 | bind steals live socket | y | should_serve_a_real_unix_socket_end_to_end |
| Z1 | time.rs:64 | leap second rejected | n | |
| Z2 | time.rs:85 | offset sign flipped | y | should_agree_on_one_instant_when_written_with_different_offsets |
| Z3 | time.rs:15 | Feb year shift | y | should_round_trip_through_format_at_millisecond_precision |

## D. Test quality issues

**Tests that can never fail**
- **`model.rs` unit tests:** `should_report_stopped_when_the_last_turn_was_interrupted`, `should_stay_calm_when_an_agent_has_never_been_observed` and `should_treat_a_question_gate_as_wanting_a_human_like_an_approval` only pattern-match values they just built. None of them calls `phase()`.
- **t10 `should_not_wake_subscribers_when_an_input_changes_nothing_visible`:** the core task is awaited, which drops the watch sender. `has_changed()` then returns `Err`, and `unwrap_or(false)` makes the assertion always pass; D1 confirms it.
- **t12 `should_wait_for_a_transcript_that_does_not_exist_yet`:** the first half, "nothing received", is trivially true.
- **t05 `should_attribute_interrupts_denials_by_the_user…`:** asserts only the count (5). The list of event kinds it builds is never compared, so swapping which events are user-caused passes.
- **t09 `should_render_equal_views_for_an_unchanged_board`:** true of any pure function.

**Tests that hang or depend on timing**
- **t10 hangs instead of failing:** `view.changed().await` has no timeout, and paused time never advances because there are no deadlines. Under E3 the binary ran for more than 12 minutes. Wrap it in `tokio::time::timeout`.
- **Real sleeps with small margins:** t12 watcher tests (10 ms poll, 50 ms sleeps) and t02 `should_report_io_when_the_daemon_never_answers` (100 ms timeout against a 400 ms sleep). Both are reasonable but will be slower and riskier under load.
- **`should_fold_a_full_board_far_inside_a_frame`:** measured 33.7 µs against a 500 µs limit in a debug build. About 15× headroom, so only a risk on a heavily loaded machine.
- **`/tmp` use:** t02_forward_golden and t11 put sockets in `std::env::temp_dir()`, i.e. `/tmp`, despite the harness's "not under /tmp" rule. t11's directory is never removed.

**Fixtures that don't match real payloads**
- **`pre_tool_use_ask_subagent.json`:** uses `agent_id: "agent-child"`. Survey 09:142 says the hook `agent_id` arrives bare, and t03 pins the prefixed form.
- **`stop_with_waits.json`:** the element shape is inferred, not recorded (survey 14, open item 16).
- **`UserPromptSubmit source:"system"` → TaskNotification:** unverified (open question 5) and untested.
- **No statusline fixture** exists at all.
- **Transcript fixture:** hand-built ids; no `<bash-` or `<user-memory-input` lines; no failed-turn line without `quotaLimits`.
- **Reducer tests skip the adapter:** they build events directly with `common::question`, which has `hash: None`, while the real adapter sends `Some(hash)`.

**Duplicated tests.** Unit tests repeat the integration tests almost one for one:
- `tail.rs` duplicates t04_tail_golden (3 tests).
- `hash.rs` duplicates t03 (2).
- `frame.rs` duplicates t02_errors (2).
- `names.rs` duplicates t01 and t02_errors (4).
- `proc.rs` duplicates t06 (2).
- `forward.rs` duplicates t03 scrub (1).
- `json.rs` duplicates t03 (2).
- `paths.rs` duplicates t01 (2).
- Within `model.rs`, `should_paint_grey_for_an_unobserved_agent_rather_than_green` overlaps `should_rank_severity_so_that_only_attention_is_an_alert`.

## E. Spec coverage

| Rule or row | Pinned? | Test / note |
|---|---|---|
| R1 agent escalation waits | yes | should_delay_escalation_when_the_agent_caused_it_and_evidence_is_inferred; should_hold_the_screen_while_an_inferred_turn_failure_serves_its_dwell |
| R1 user escalation immediate | partially | always paired with Confirmed; S2 survives |
| R1 de-escalation immediate | yes | should_de_escalate_immediately_when_the_agent_caused_it |
| R2 dwell by evidence | yes | S3, S1 and S5 are all killed (0 / 10 s / 6 s). The 6 s is fixed, not tied to the real refresh interval. |
| R3 unowned never Attention | partially | should_render_an_unowned_gate_grey_never_red_even_mid_turn; M9, S6, H4 survive |
| R4 liberal lifting | partially | settle, Stop, next prompt, denial with id are pinned; E24–E26 and H5 survive |
| R5 hold when ambiguous | partially | should_keep_both_parked_prompts_when_an_unseen_call_settles_ambiguously; should_restart_the_dwell_when_the_pending_phase_changes |
| R6 publish coalescing | no | D1 and D2 survive (the D1 test can never fail) |
| Duplicate event no-op | partially | turn, dispatch, prompt, Died, cwd, subagent pinned; E8, E16 and SessionStarted not |
| Late settle for unknown id | partially | should_publish_nothing_when_an_unknown_call_settles is mid-turn; the Idle-after-Stop case is untested |
| Stop before late PostToolUse abandons calls | partially | should_go_idle_and_abandon_open_calls_when_the_turn_ends; the late settle afterwards is untested |
| Stale snapshot vs subagents | yes | should_drop_a_panel_taken_before_the_last_event (equal instant untested, M1) |
| Stale snapshot vs dead agents | partially | only scanner order, via should_emit_the_sweep_then_a_death_for_each_vanished_process; reducer fails (A4) |
| /clear race | partially | should_ignore_a_late_event_for_the_previous_session_after_clear; late SessionStart fails (A5) |
| Reconstruction never Attention | partially | adapter can't emit a gate (fixture sequence test), and restart shows grey (should_show_unknown_for_a_discovered_agent_until_it_is_heard_from); nothing stops inferred Attention (S6) |
| Suspend gap | no | not implemented anywhere in `src` |
| Daemon down/wedged, bounded + exit 0 | partially | should_report_connect_when_no_daemon_is_listening; should_report_io_when_the_daemon_never_answers…; exit 0 untested |
| Hook storm, bounded inbox sheds | no | `offer` is untested |
| Statusline on render path, exec wrapped script | no | not implemented |
| Out-of-order hooks, join both ways | yes | should_gate_the_running_call_when_a_prompt_arrives_after_it; should_park_a_prompt_that_arrives_first_and_join_it_when_the_call_arrives |
| Removal deduped by membership | yes | should_remove_the_agent_forget_it_and_record_it_when_it_dies |
| Recycled pid | yes | should_treat_a_recycled_pid_as_a_death_and_a_birth |
| Transcript replaced on /clear, re-watch | partially | WatchTranscript emitted and Watchers::start pinned; the effect runner is untested |
| Truncation, re-read | yes | should_restart_from_zero_and_drop_the_carry_when_the_file_shrinks (tail level only) |
| Two publishes, newest wins | no | `publish::run` untested |
| Federation, unconfirmed goes grey | yes | should_paint_an_unconfirmed_remote_row_grey_whatever_it_last_said |
| Placement timeouts, app-server, pidfd, bar restarts | n/a | not in this build |