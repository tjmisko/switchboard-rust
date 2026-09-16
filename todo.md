# Switchboard redesign: getting started

Decisions are tracked in the [Switchboard Decision Record](https://claude.ai/code/artifact/da382820-40eb-4256-b23c-acbe74fed282).

- [x] Install the capture harness; recording rechecked September 14
- [ ] Run the marked scenarios in `tools/hookcap/PROTOCOL.md`; normal-use data is collected, but there are still no scenario marks
- [ ] Review [September 14 capture findings](tools/hookcap/FINDINGS-2026-09-14.md): Codex approval hooks include requests not shown to the user; observed `PreToolUse` precedes `PermissionRequest`; Claude waiting is verified for questions only
- [x] Work the fix queue in [docs/13-capture-fix-handoff.md](docs/13-capture-fix-handoff.md): F1 limits parsing, F2 actor routing, F3 the join, F4 subagent staleness, F5 naming, F6 waits, F7 origin (2026-09-15; F8 waits for the Codex adapter, F9 for captured `error` values)
- [x] Turn the four failing tests in `tests/t13_capture_findings.rs` green and refresh the inferred fixtures from captured payloads (2026-09-15; propagated to `exercises`, `main`, task sheets and the guide, all uncommitted)
- [ ] Commit the capture fixes: golden as atomic commits, `exercises` as one scaffold commit, `main` as one squashed commit the learner merges
- [ ] Settle whether `UserPromptSubmit.turn_id` (175 of 263 captured prompts, no `prompt_id`) equals the transcript's `promptId` before `prompt_key` reads it
- [ ] Run `analyze.py`, settle O4 (subagent signals) and O5 (Claude's status file) from the data, then `install.py uninstall`
- [ ] Decide O1–O3: who owns Codex approvals, what happens after a daemon restart, output schema
- [ ] Confirm D1–D3: providers as a table of functions, vocabulary tolerance, capabilities kept out of the model
- [ ] Pick OS crates (`notify`, `rustix`), and decide whether `serde` derive and `thiserror` join the course
- [ ] Write the decisions up as `docs/12-redesign-decisions.md`, and commit it along with `tools/hookcap/`
- [ ] Reshape `model.rs`: `ProviderId`, `Other` variants, `Verdict`, `Now`, `Input::Replay`, `Board.dead`
- [ ] Copy `docs/audit/repro/adv_bugs.rs` into `tests/`, then fix high-severity bugs #1–#4 against the new shape
- [ ] Build the provider table and the `cfg`-selected platform module, then the Codex adapter behind them
- [ ] Re-cut the course (scaffold, task sheets, guide, hidden suites) and rerun `mutate.py`, aiming for at least 85% of mutations killed
