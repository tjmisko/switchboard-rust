# 13 — Handoff: fixing what the capture found

Written 2026-09-15. Slot 12 is reserved for the redesign decisions the todo list
names; this document is only about defects the signal capture demonstrated.

Inputs:
- `tools/hookcap/FINDINGS-2026-09-15.md` — evidence, counts and citations.
- `tools/hookcap/FINDINGS-2026-09-14.md` — the Codex ordering correction.
- `tests/t13_capture_findings.rs` — four failing tests, written against the real
  payload shapes. Run `cargo test --offline --test t13_capture_findings`.
  All four fail today; each fix below names the test that must turn green.

Nothing here is committed. `tools/` and `todo.md` are untracked on `golden`.

## Status, 2026-09-15 evening: F1–F8 done, F9 open

All four tests in `tests/t13_capture_findings.rs` are green, along with the
whole suite (266 tests) and `cargo clippy --all-targets`. The same changes
are applied, uncommitted, in all five places the rules below name: `golden`,
`exercises`, `main` (working tree, no learner commits existed), the task
sheets (T03, T06, T08, T09, `exercises/README.md` item 11) and the guide
(`~/.cache/sbr-audit/guide-src/`, rebuilt into `~/Guides/Building Switchboard
in Rust.html`; every rewritten checkpoint filter re-verified with
`cargo test --test <file> -- --list <filter>`).

| # | Done | Where | Pinned by |
|---|---|---|---|
| F1 | per-window `rate_limits` parsed; status derived from the fullest window (≥100 Rejected, ≥80 Warning); documented `status` shape still preferred when present | `ingest/claude/statusline.rs` | `t13 should_read_rate_limits_from_the_per_window_status_line_shape`, three unit tests in the module, fixtures under `tests/fixtures/claude/statusline/` |
| F2 | `transition` routes a subagent's event to `subagent_transition` before matching on kind; a child's failed turn marks its row `Failed`, clears its gate, and touches nothing on the agent; every child signal refreshes `last_seen` | `reduce/event.rs` | `t08 …when_a_subagents_turn_fails`, `…subagents_tool_is_interrupted`, `…ignore_a_subagents_session_compaction_and_cwd_events`, `…subagents_gate_when_its_permission_is_settled`; `t13 …when_a_subagents_turn_fails` |
| F3 | a call-less ask prefers a *running* `(tool, hash)` match; a *blocked* match is the same prompt said twice and is a no-op; otherwise park | `reduce/event.rs` `ask` | `t08 …as_the_same_prompt`, `…two_identical_questions_open…`; `t13 …announced_by_two_hooks` |
| F4 | `SUBAGENT_STALE` = 30 min; `expire` removes an ungated row past it whoever owns the table; `deadlines` includes it | `reduce/mod.rs`, `reduce/tick.rs` | `t09 …for_the_stale_cap`, `…gated_subagent_row_past_the_stale_cap`, `…panel_rows_too…`; `t13 …goes_stale` |
| F5 | `proc::name_from_file` under the same `written_by` guard as the session; `ProcessRow.name`; discovery sets `title`, a later sweep with a different name renames and publishes, no name leaves it | `proc.rs`, `model.rs`, `reduce/snapshot.rs` | `t06 …display_name_from_the_session_file…`, sweep assertions; `t09 …follow_a_rename` |
| F6 | `WaitKind::{Workflow, Other}`; `wait_kind` maps `shell`/`monitor`/`workflow` exactly and carries anything else as `Other` | `model.rs`, `ingest/claude/hook.rs` | `t03 …stop_reports_them` (refreshed fixture), `…unknown_background_task_type_as_other` |
| F7 | `Origin::Unknown`; `prompt_origin(None)` is `Unknown` with `Cause::Agent`; `source`, when present, still maps as documented. The transcript side is unchanged: 35 of 53 human prompt lines in local transcripts carry no `origin` at all, so absent → human stays right there | `model.rs`, `ingest/claude/hook.rs` | `t03 …when_a_prompt_has_no_source` |
| F8 | the Codex hook adapter exists (`ingest/codex/hook.rs`): the forwarder hashes the command alone for Codex and names the provider on the frame, `decode_request` reads it (default Claude), the listener dispatches by it, `classify` recognises the Codex TUI, turns key on `turn_id`, `Interrupt` is the user-caused stop, a `PermissionRequest` gate is `Ownership::Unknown` and renders grey, and no transcript is watched for a Codex agent | `ingest/codex/`, `ingest/forward.rs`, `ingest/frame.rs`, `daemon/rpc.rs`, `proc.rs`, `bin/switchboard-hook.rs`, `reduce/mod.rs` | `tests/t14_codex.rs` (13 tests), `t06 …interactive_codex_tui…`, `t11 …codex_adapter…`, unit tests in `frame.rs`, `forward.rs` and the forwarder binary |
| F9 | not started: all 56 `StopFailure` records still show `<str:10>`; no record has arrived since the scrub fix | | |

Fixtures refreshed or added from captured shapes: `stop_with_waits.json`
(`{id, type, status, description, name?, command?}`),
`pre_tool_use_ask_subagent.json` (bare `agent_id`, t03 assertion updated),
`stop_failure_subagent.json`, the `pre_tool_use_ask.json` /
`permission_request_ask.json` pair, `session_start_startup.json` (no title),
`user_prompt_submit_no_source.json`, and three status-line payloads.
`session_start_clear.json` and `user_prompt_submit.json` keep the documented
`session_title` / `source` fields on purpose: they pin the tolerance path.

Decisions taken while fixing (the open questions at the end are answered
in place):

1. **F4 cap: 30 minutes.** Matches `WAIT_EXPIRY`; both bound a claim whose
   source stopped repeating, and being wrong either way moves the chip
   between two greens. A row holding a gate never expires, because a tick
   never clears a live gate; the settle that answers it refreshes the row.
   Panel-owned rows expire too: a panel that went quiet is not authoritative.
2. **F2: a failed child shows on its row** (`SubagentStatus::Failed`), which
   the tooltip's subagent count already reflects. No new `Reason` variant.
3. **F5: the name is taken now.** Only `name`, under the existing guard. The
   rest of the session file stays O5.
4. **Tests:** the unit-level rules went into `t03`, `t06`, `t08`, `t09`
   (provided suites, so the scaffold carries them); `t13_capture_findings.rs`
   stays golden-only as the end-to-end file that runs real payload shapes
   through adapter and reducer. Names were kept where they already stated
   the behaviour.
5. **Documents corrected** (golden, exercises and main): `docs/survey/10`
   (the 32 s claim, now 60 ms median / 101 ms max), `docs/survey/14`
   (`session_title` and `source` never captured), `docs/05` (`limits_feed`
   row), and the signals guide (Codex `PermissionRequest` has no
   `tool_use_id`; the two Claude rows; the capability row).

Found while fixing, not in the queue:

- **Retracted the same evening:** an unfiltered tally read 175 prompts with
  `turn_id` and no `prompt_id` as a new Claude shape. Filtered by provider
  they are all Codex (183 of 183 Codex prompts carry `turn_id`; 101 of 101
  Claude prompts carry `prompt_id`). Codex turns are keyed by `turn_id` in
  its own adapter; `prompt_key` on the Claude side is right as it is.
- The mutation driver at `~/.cache/sbr-audit/mutate.py` had six anchors in
  code the fixes rewrote. `~/.cache/sbr-audit/mutate_capture.py` retires
  them, adds replacements plus 17 mutations against the new rules, and
  writes to `mutations-2026-09-15.jsonl`. Results are in the verification
  section below.

## Rules that still bind

From `docs/11-golden-audit-handoff.md`, unchanged:

- A public signature change must land in five places: golden, the `exercises`
  branch, `main` (as a commit the learner merges), the task sheet, and the guide.
- `main` only ever receives squashed commits; never merge golden history into it.
- Every learner-owned location keeps a `todo!()` or an agreed stub.
- Dependencies are `serde_json` and `tokio` until decision D4 says otherwise.
- Tests are named `should_<behaviour>_when_<condition>`.

Two of the fixes below are also decisions in the
[decision record](https://claude.ai/code/artifact/da382820-40eb-4256-b23c-acbe74fed282):
F2 is the concrete case for D8 (apply an event to the actor it names) and F3 for
D3 (one join rule, richest evidence wins). Fixing them settles those rows by
construction.

## Queue, in the order I would do them

| # | Defect | Where | Decision first? | Test to turn green | Size |
|---|---|---|---|---|---|
| F1 | Status-line limits never parse | `ingest/claude/statusline.rs` | no | `should_read_rate_limits_from_the_status_line_shape_2_1_269_sends` | ~40 lines |
| F2 | A subagent's failed turn blocks the parent | `reduce/event.rs` | small rule | `should_keep_the_agent_working_when_a_subagents_turn_fails` | ~30 lines |
| F3 | An answered question stays red | `reduce/event.rs` | small rule | `should_leave_attention_when_an_answered_question_was_announced_by_two_hooks` | ~25 lines |
| F4 | Delegating never expires | `reduce/tick.rs`, `model.rs` | constant | `should_stop_claiming_delegation_when_a_subagent_row_goes_stale` | ~30 lines |
| F5 | Session naming lost | `proc.rs`, `reduce/snapshot.rs` | no | new | ~40 lines |
| F6 | Waits vocabulary too narrow | `ingest/claude/hook.rs`, `model.rs` | no | new | ~15 lines |
| F7 | Every prompt counts as user-typed | `ingest/claude/transcript.rs` | no | new | ~20 lines |
| F8 | Codex join cannot match | `ingest/`, `model.rs` | yes, with D3 | new | ~60 lines |
| F9 | Confirm `StopFailure` classes | fixtures only | no | new | ~10 lines |

F1 first because it is self-contained and unblocks every limits behaviour,
including the expiry that audit bug #1 describes. F2 and F3 next because they are
the two wrong colours an operator sees daily. F4 and F5 are small and visible.
F8 waits for the Codex adapter to exist.

## F1 — Status-line limits

**Now:** the adapter requires `rate_limits.status` and returns nothing without it
(`statusline.rs:23`). 408 of 423 captured payloads carry per-window objects
instead.

**Change:** read the windows, and derive the status rather than expecting one.

```rust
// rate_limits: { five_hour: { used_percentage, resets_at }, seven_day: { … } }
// Take the window closest to its cap; it is the one that will stop the agent.
let worst = ["five_hour", "seven_day", "seven_day_opus", "seven_day_sonnet", "overage"]
    .iter()
    .filter_map(|name| r.get(*name).map(|w| (window_kind(name), w)))
    .filter_map(|(kind, w)| Some((kind, u64_at(w, "used_percentage")?, u64_at(w, "resets_at"))))
    .max_by_key(|(_, used, _)| *used);
```

Keep reading the documented `status` / `resetsAt` / `utilization` shape when it is
present: both are "richest evidence wins", and an older or newer harness may send
either. Status derives from the percentage (100 or more is `Rejected`, 80 or more
`Warning`), which is weaker than a harness-reported `rejected` and should be
marked `Confidence::Snapshot`, not `Confirmed`.

**Note:** no captured payload reports a rejected account, so `Blocked` from a
limit is still unobserved end to end. `StopFailure` remains the only measured
path into it.

## F2 — Apply an event to the actor it names

**Now:** `TurnEnded` is handled identically whoever sent it (`event.rs:129`), so a
child's `StopFailure` ends the parent's turn and paints it `Blocked`. 47 of 56
captured `StopFailure` events came from a child.

**Change:** route by actor at the top of `transition`.

```rust
// A subagent's turn is not the agent's turn. Claude reports a child's lifecycle
// through SubagentStart/Stop; its turn events refine that child's row only.
if let Actor::Subagent(id) = &event.actor {
    return subagent_transition(agent, id, event, now);
}
```

`subagent_transition` handles the arms a child can legitimately send: turn ended
(record the outcome on the row: a failed child is a fact worth showing in the
tooltip, and by R3 it may raise the agent's phase only through a gate, never by
itself), tool dispatched and settled (per-call gates, audit #7), and permission
asked and settled. Everything else is a no-op.

**Watch:** `PostToolUse` and `PreToolUse` carry `agent_id` for about 97% of tool
events on this machine, so this path is the common one, not the exception. Keep
`should_raise_attention_from_a_subagent_gate_and_keep_it_through_the_agents_stop`
green: the agent's own `Stop` must still not clear a child's gate.

## F3 — One join rule

**Now:** a question tool produces two prompts, and the second parks forever
(`event.rs:420`, `event.rs:351`).

**Change:** when an ask carries no call id, match it against calls that are
already blocked as well as running, and treat a match as a duplicate.

```rust
// A prompt with no id may describe a call we have already gated: Claude sends
// PreToolUse (with the id) and PermissionRequest (without) for the same question.
if let Activity::Turn { tools, .. } = &mut agent.activity {
    if let Some(t) = tools.values_mut().find(|t| t.name == *tool && hash_admits(hash, t.hash)) {
        return match &t.gate {
            Gate::Blocked { .. } => false,          // the same prompt, said twice
            Gate::Running => { t.gate = gate; true }
        };
    }
}
```

This is D3's rule in miniature: the id is the richer evidence, the hash is the
fallback, and the poorer signal must never contradict the richer one. It also
removes the reason `lift_sole_unjoined` exists for this case.

**Check while here:** `park` dedupes on `(actor, tool, hash)`, so two genuinely
different questions with identical inputs in one turn still park once. That is
the audit's E22 gap and needs a test either way.

## F4 — A subagent row must be able to go stale

**Now:** rows are removed only by `SubagentEnded` (`event.rs:214`); `tick` has no
expiry for them. 54 captured children never reported a stop.

**Change:** expire a row whose `last_seen` is older than a cap, and include that
deadline in `next_deadline`. Every subagent signal already refreshes `last_seen`,
so a live child with hooks or a panel refresh never expires.

**Decide the constant.** The panel refreshes every 5 s when it runs; hook traffic
from a working child is far more frequent. A cap of 30 minutes matches
`WAIT_EXPIRY` and would have expired all 54. Any value between 10 and 30 minutes
is defensible; it only ever removes a claim we can no longer support.

## F5 — Read the name the harness already wrote

**Now:** `Agent.title` comes only from `SessionStart.session_title`
(`hook.rs:75`), which 0 of 49 captured payloads carry. The name the operator
recognises sits in `~/.claude/sessions/<pid>.json` as `name`, which `proc.rs:129`
opens and ignores.

**Change:** return the name alongside the session from the session file, carry it
on `ProcessRow`, and let `discover` set it. Keep `session_title` as the richer
source when a harness sends it.

**Scope note:** the same file carries `status`, `waitingFor`, `kind` and
`tempo`. Using those is decision O5 and is deliberately not part of this fix;
this is only the name, which is unambiguous and already proven by the Go daemon.

## F6 — Waits vocabulary

Captured `background_tasks[].type`: `workflow`, `monitor`, `shell`. Add
`WaitKind::Workflow`, map `shell` explicitly, and keep an `Other` arm per D2 so a
new type is carried rather than guessed. `session_crons` has never been observed;
leave the parser and mark the shape unverified in the doc comment.

## F7 — Origin of a prompt

`UserPromptSubmit` has no `source` (83 of 83), so every prompt is treated as
typed by the operator and escalates with no dwell. The transcript's `origin.kind`
is the only discriminator. Move origin to the transcript adapter, and have the
hook emit `TurnStarted` with an unknown origin that the transcript refines.

**Cost-model consequence:** until the transcript line arrives, a machine
wake-up is indistinguishable from a typed prompt. Since a turn starting is a
de-escalation to `Working`, being wrong here is cheap; note it and move on.

## F8 — The Codex join

Blocked on the Codex adapter existing. When it lands: hash the command alone, not
the whole `tool_input` (the request adds a `description` the call lacks), and
expect `PreToolUse` 13-60 ms *before* `PermissionRequest`. Do not implement "a
later `PreToolUse` means approved" — the September 14 note retracted it.
Ownership stays `Unknown` from hooks; O1 decides what that renders as.

The model change this implies: the hash a prompt joins on is provider-specific,
so `InputHash` should be computed by the adapter over a provider-chosen subset,
and the reducer must not assume both sides hashed the same fields.

## F9 — Confirm the failure classes

`StopFailure.error` was stored as a shape marker until today; all 56 records show
`<str:10>`, consistent with `rate_limit` but unproven. After a day of capture,
tally the values and check them against `ErrorClass::from_wire` (`names.rs:193`).
Add a fixture per observed value.

## Fixtures to refresh from the capture

The audit flagged several fixtures as inferred. These now have real shapes:

- `statusline` — no fixture exists at all; add the per-window payload.
- `stop_with_waits.json` — replace the inferred element shape with the captured
  one (`{id, type, status, description, name?, command?}`).
- a `StopFailure` from a child (`agent_id` present) — the F2 case.
- the `AskUserQuestion` pair, `PreToolUse` then call-less `PermissionRequest` —
  the F3 case.
- `pre_tool_use_ask_subagent.json` uses a prefixed `agent_id`; every captured `agent_id`
  is bare. Fix the fixture and the t03 assertion.
- `session_start` without `session_title` — the F5 case.

## Verification

1. `cargo test --offline` and `cargo clippy --all-targets` clean.
2. The four tests in `tests/t13_capture_findings.rs` green, with each renamed to
   state the behaviour it now guarantees rather than the defect it found.
3. Re-run the mutation driver (`~/.cache/sbr-audit/mutate.py`); the suggested bar
   from handoff 11 is 85% of mutations killed.
   **Done 2026-09-15 with `mutate_capture.py`:** 112 mutations, 67 killed (60%).
   The 22 mutations against the new rules are all killed (four needed an extra
   test each: the subagent-side join, a re-ask on a gated row, a stale file's
   name, the `system` source). The 90 pre-existing mutations sit at 45 killed
   (50%), against 43 of 96 on September 12; the survivors are handoff 11's
   Queue B and are untouched by this queue. Results:
   `~/.cache/sbr-audit/mutations-2026-09-15.jsonl`.
4. Leave the capture running for a day after the fixes and re-run
   `tools/hookcap/analyze.py`: F1, F6 and F9 are confirmed by new records, not by
   unit tests.

## Documents these findings contradict

- The guide's ingestion table says Codex `PermissionRequest` carries
  `tool_use_id`. It does not (0 of 31).
- `docs/survey/10` says a transcript `end_turn` can precede the turn's real end
  by up to 32 s. Measured at 60 ms median, 101 ms max over 30 pairs.
- The guide lists `session_title` as the naming source, and
  `UserPromptSubmit.source` as the human/machine discriminator. Neither field
  appears in 2.1.269 or 2.1.270 payloads.
- The capability table's `limits_feed` row should describe the per-window shape.

## Open questions

Answered on 2026-09-15; see the status section at the top.

1. F4's stale cap: 10 minutes or 30? **30.**
2. F2: should a failed child show in the parent's tooltip, or only in the
   panel? **On its row, as `Failed`; the tooltip's subagent count follows.**
3. F5: take the name from the session file now, or wait for the whole O5
   decision about that file? **Now, name only.**
4. Should the four tests move into `tests/t08_join.rs` and
   `t09_snapshot_tick.rs`? **Both: unit rules in the provided suites,
   end-to-end stories stay in `t13`.**

Still open:

5. ~~Does `UserPromptSubmit.turn_id` equal the transcript's `promptId`?~~
   Withdrawn: those prompts were Codex's. See the retraction above.
6. The mutation bar. The survivors from handoff 11 (Queue B) are untouched
   by this queue; the kill rate on the 17 new mutations is the measure of
   these fixes, the overall rate is the measure of the remaining audit work.

## Course impact of the Codex adapter (not yet propagated)

The adapter landed on `golden` only. It changes shapes the scaffold compiles
against, so the five-place rule is owed before the next course cut:

- `HookFrame` gained `provider: Provider` and `decode_request` reads an
  optional `provider` (T02: one more row in the frame table).
- `forward::scrub_for(provider, …)` is new; `scrub` delegates to it (T03).
- `classify` recognises the Codex TUI (T06: a second rule set).
- The listener dispatches by provider (T11), and `Effects::watch` replaces
  four hand-built `WatchTranscript` pushes (T07/T09).
- `tests/t14_codex.rs` and `tests/fixtures/codex/hooks/` are the natural
  material for a T14 "the second provider" task once the provider table (D1)
  is decided.
