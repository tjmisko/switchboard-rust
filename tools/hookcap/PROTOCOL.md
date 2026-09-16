# Signal-capture experiment

Settles the questions the design is currently guessing at (`docs/product/10-open-items.md` A1,
`docs/product/survey/14` §5, and the open decisions O1, O2 and O4) by recording what Claude
Code and Codex actually emit on this machine.

## What it records

All output goes to `~/.local/state/switchboard/hookcap/<day>/`.

| Stream | Source | Content |
|---|---|---|
| `hooks.jsonl` | every Claude and Codex hook event, beside the existing `switchboard-ctl` hooks | ids, enums, short fields; the agent's run state, CPU, children, and its `sessions/<pid>.json` status at that instant |
| `statusline.jsonl` | the `statusLine` command, teed before your `claude-status` runs | API time, rate limits, context % |
| `subagents.jsonl` | `subagentStatusLine` (prints nothing, so the panel is unchanged) | task ids, types, statuses |
| `sampler.jsonl` | a background sampler | every change to `sessions/<pid>.json` status fields (250 ms), plus `/proc` state for each agent (2 s) |

**Privacy:** prompts, tool inputs, tool output and assistant text are never stored. Their fields keep their shape, but each value becomes a marker like `<str:42>`. Tool inputs are hashed so the prompt join can be checked.

**Cost:** about 15 ms per hook, synchronous. PreToolUse is registered with no matcher, so each tool call pays it twice.

## Run it

```sh
python3 tools/hookcap/install.py install     # backs up both configs first
python3 tools/hookcap/install.py status
# … work normally for a day, and run the scenarios below once …
python3 ~/.local/share/switchboard-hookcap/analyze.py
python3 tools/hookcap/install.py uninstall   # recordings are kept
```

New sessions pick the hooks up. Restart long-running sessions, or check `/hooks`.

## Scenarios (about 20 minutes)

Before each scenario, mark the recording so the analyzer can line events up with what you did:

```sh
~/.local/share/switchboard-hookcap/hookcap mark "2 decline a prompt"
```

Scenarios that need a fresh throwaway session: open one with `claude` in a scratch directory.

| # | Do this | Settles |
|---|---|---|
| 1 | In a throwaway Claude session using default permissions, trigger a benign command that actually shows an approval prompt. Leave it open for 60 s, then approve. Auto mode and already-allowed commands may produce no prompt. | Does `status: waiting` track a command approval prompt, and what does the process look like while waiting (O2, O5)? |
| 2 | Trigger a prompt and decline it. | Does `PermissionDenied` fire on a human decline? |
| 3 | Ask for a long essay. Press Esc once text is streaming, with no tool running. | Does `Stop` fire on Esc with no tool open? Is `interruptedMessageId` present? |
| 4 | Send a prompt and press Esc immediately, before any output. | Does anything fire? (audit #5: 24/51 of these lack the id) |
| 5 | Run `sleep 30` through Bash and press Esc while it runs. | `PostToolUseFailure{is_interrupt}` baseline |
| 6 | Run `/clear` in a session with some history. | Does `sessions/<pid>.json` follow the new session id, and how fast (audit #6)? |
| 7 | Ask for two parallel subagents doing slow work. Mark `7 panel closed`, keep the panel closed for 30 s. Then mark `7 panel open` and open it. | Does `subagentStatusLine` fire with the panel closed (O4)? Task types and statuses |
| 8 | Ask for a background `sleep 300` (run_in_background) and a `/loop` or cron, then let the turn end. | Shapes of `Stop.background_tasks[]` and `session_crons[]` |
| 9 | Paste an image with a short prompt. | Image prompts start a turn (audit #8) |
| 10 | From a shell: `claude -p "say hi"`. | Hooks and session-file `kind` for headless runs (audit #2) |
| 11 | Codex: ask for a command that needs approval. At the prompt, press Ctrl-C. | Does `Interrupt` fire during an approval prompt? |
| 12 | Codex: let a session sit idle for 35 minutes without exiting. | Does `SessionEnd` fire on an idle timer? |
| 13 | Codex: run ten benign commands that need approval, sequentially in one test session. Mark each visible prompt and its response using the convention below. | Which requests reach a human, and whether a timing grace can distinguish them (O1) |
| 14 | Run `/compact` manually. | `PreCompact`/`PostCompact` with trigger `manual` |

### Scenario 13: distinguish automatic review from a visible prompt

Use another terminal for marks. Pick a session label (here `c1`) and a new trial
number for each command. Avoid simultaneous test requests or repeated identical
commands within a turn: command hashes identify content, not a unique invocation.

```sh
~/.local/share/switchboard-hookcap/hookcap mark "13 c1 trial-01 begin"
# Only if an approval prompt actually appears:
~/.local/share/switchboard-hookcap/hookcap mark "13 c1 trial-01 visible"
# Immediately before answering it (or use decline / ctrl-c):
~/.local/share/switchboard-hookcap/hookcap mark "13 c1 trial-01 approve"
# After the result arrives:
~/.local/share/switchboard-hookcap/hookcap mark "13 c1 trial-01 completed"
```

If the request finishes without showing a prompt, mark `13 c1 trial-01 no-prompt`
afterward. Keep the existing automatic-review setting for these trials. A
`PermissionRequest` hook alone does not establish that a human saw a prompt;
request-to-`PostToolUse` time includes execution, and unrelated tool activity is
not a resolution. The analyzer prints the mark timeline for manual comparison;
these labels do not automatically associate a mark with a session or command.

## Reading the result

`analyze.py` prints one section per question, with the evidence and a one-line reading.
A section with no data says so. An event that was never exercised is not an event
that doesn't fire, so check the scenario timeline at the end before concluding anything.

See [the September 14 findings](FINDINGS-2026-09-14.md) for the first measured
waiting intervals and corrections to the initial Codex interpretation.
