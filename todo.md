# Switchboard redesign: getting started

Decisions are tracked in the [Switchboard Decision Record](https://claude.ai/code/artifact/da382820-40eb-4256-b23c-acbe74fed282).

- [x] Install the capture harness; recording rechecked September 14
- [ ] Run the marked scenarios in `tools/hookcap/PROTOCOL.md`; normal-use data is collected, but there are still no scenario marks
- [ ] Review [September 14 capture findings](tools/hookcap/FINDINGS-2026-09-14.md): Codex approval hooks include requests not shown to the user; observed `PreToolUse` precedes `PermissionRequest`; Claude waiting is verified for questions only
- [x] Work the fix queue in [docs/product/13-capture-fix-handoff.md](docs/product/13-capture-fix-handoff.md): F1 limits parsing, F2 actor routing, F3 the join, F4 subagent staleness, F5 naming, F6 waits, F7 origin (2026-09-15; F8 done with the Codex adapter the same evening, F9 waits for captured `error` values)
- [x] Turn the four failing tests in `tests/t13_capture_findings.rs` green and refresh the inferred fixtures from captured payloads (2026-09-15; propagated to `exercises`, `main`, task sheets and the guide, all uncommitted)
- [x] Commit the capture fixes: golden as atomic commits, `exercises` as one scaffold commit, `main` as one squashed commit the learner merges (2026-09-15)
- [x] ~~Settle whether `UserPromptSubmit.turn_id` equals the transcript's `promptId`~~ Withdrawn: those prompts were all Codex, whose adapter keys turns by `turn_id` (2026-09-15)
- [x] Codex hook adapter built (2026-09-15) and parked on the `codex` branch (2026-09-16): `ingest/codex/`, provider-named frames, command-only join hash, `classify` for the TUI. Not propagated to the course on purpose; see docs/13 *Course impact*
- [x] Dry-run the course with three learner agents, consolidate, and apply the Tier 1 and 2 fixes to golden, exercises and main (2026-09-16; docs/product/14). Tier 3 (T05 before T07, T02 error impls to their consumers, a usage mini-task, a fourth async task) undecided; the rendered guide is owed
- [x] Freeze the course as Claude-only, hooks-only (2026-09-16): no scaffold-shaping change lands on `golden`, `exercises` or `main` until T15 is graded. Product work below waits, or becomes the advanced course (T14+)
- [ ] Run `analyze.py`, settle O4 (subagent signals) and O5 (Claude's status file) from the data, then `install.py uninstall`
- [ ] Decide O1–O3: who owns Codex approvals, what happens after a daemon restart, output schema
- [ ] Confirm D1–D3: providers as a table of functions, vocabulary tolerance, capabilities kept out of the model
- [ ] Pick OS crates (`notify`, `rustix`), and decide whether `serde` derive and `thiserror` join the course
- [ ] Write the decisions up as `docs/product/12-redesign-decisions.md`, and commit it along with `tools/hookcap/`
- [ ] Reshape `model.rs`: `ProviderId`, `Other` variants, `Verdict`, `Now`, `Input::Replay`, `Board.dead`
- [ ] Copy `docs/product/audit/repro/adv_bugs.rs` into `tests/`, then fix high-severity bugs #1–#4 against the new shape
- [ ] After T15: build the provider table and the `cfg`-selected platform module as the advanced course, then move the Codex hook adapter (on `codex`) behind them
- [ ] Re-cut the course (scaffold, task sheets, guide, hidden suites) and rerun `mutate.py`, aiming for at least 85% of mutations killed
