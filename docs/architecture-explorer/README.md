# Switchboard architecture explorer

Open [index.html](index.html) in a browser. It is a standalone HTML file: all CSS,
JavaScript, diagrams, and reference data are embedded. No server, npm install,
network access, or running daemon is needed.

The explorer is grounded in the Rust source at golden revision `17ff0c9` and the
Signals guide. It distinguishes the currently wired Claude/Linux pipeline from
declared but unwired models, audited defects, and redesign proposals. It does not
treat a closed provider set or full Go feature parity as the target architecture.

## Explore

1. **Follow a signal:** eight animated scenarios, 56 steps, 14 inspectable nodes.
   Play, pause, step backward, jump to a step, change speed, or expand the map.
   Each step displays the payload, agent state, effects, and source references.
2. **Inside an agent:** independently vary activity, gate ownership, limits,
   children, waits, and outcomes. See the exact priority order and experiment
   with cause, confidence, and display dwell.
3. **The Rust types:** search 20 type cards, inspect actual declarations and
   field meanings, jump from diagram nodes, and copy declarations.
4. **Tasks & channels:** experiment with a paused subscriber; inspect capacities,
   producer backpressure, the effect feedback loop, and the full output schema.
5. **The wider design:** independent local Board, remote Fleet, history,
   provider/platform boundaries, missing connections, and unresolved choices.

The diagrams show data and calls, not one task per box. All example values are
synthetic. Scenarios are explanatory traces, not a Rust interpreter or a live
monitor. Panel reconciliation is modeled in Rust but lacks a running panel source.
The experiment harness and any real recordings are not used by the page.

The page supports keyboard operation, reduced motion, and narrow screens. On a
phone, pan the system diagram horizontally. The build embeds absolute local source
links to the golden checkout, so copying `index.html` to `~/Guides/` preserves them.
Rebuild after moving the checkout or copying the explorer to another machine.

## Edit and build

| File | Purpose |
| --- | --- |
| `shell.html` | Layout and explanatory copy |
| `app.css` | Responsive dark theme |
| `app.js` | Diagram, scenario player, inspectors, and interactive models |
| `system.js` | Source-backed topology, channel contracts, architectural gaps |
| `types.js` | Rust declarations, field explanations, source anchors |
| `scenarios.js` | Eight annotated traces |
| `build.py` | Bundles the sources into the standalone `index.html` |

The explorer is maintained on `main`. From the main repository:

```bash
python3 docs/architecture-explorer/build.py
```

Source links and oracle generation use `.worktrees/golden` by default. Both Python
commands accept `--source-root /path/to/golden` for a different checkout. This keeps
the reference examples and verification tied to the implementation they describe.

Install the standalone guide with:

```bash
cp docs/architecture-explorer/index.html "$HOME/Guides/Understanding Switchboard's Architecture.html"
```

Keep current and proposed behavior distinct when updating the source data.
If a Rust type or behavior changes, update its card/scenarios, source anchors,
and the oracle before rebuilding.

## Verification

`verify-model.rs` executes the actual golden library for 288 combinations of
agent facts and 336 combinations of truth, cause, confidence, and elapsed time.
`model-parity.json` records its output. Regenerate it with:

```bash
python3 docs/architecture-explorer/refresh-oracle.py
```

`verify.mjs` uses Chromium's DevTools pipe directly, with no npm dependencies.
Run the browser checks with:

```bash
node docs/architecture-explorer/verify.mjs
```

The default executable is `/usr/bin/google-chrome`. Set `CHROME_BIN` to another
Chromium executable, including a `headless_shell`, if needed. Screenshots go to
a new `switchboard-explorer-check-*` directory under the system temporary directory.

Verified during creation:

- The JavaScript phase/display lab matches all **624 Rust oracle cases**.
- All **56 scenario steps** render their specified payload, phase, and active node.
- Playback, inspection, Escape, search, copy, dwell controls, and latest-view
  subscription behavior work in Chromium Headless Shell.
- Desktop, laptop, and 390 px mobile layouts were captured; all five mobile
  pages pass a page-overflow check. The map itself intentionally scrolls.
- All source links name existing files and in-range line numbers.
- Browser runtime reports no exceptions; reduced-motion controls work.

The oracle validates the fold and display policy, not event routing, adapters,
or daemon I/O. Scenarios were checked against source and independently reviewed;
they are not complete executable integration tests. This work does not fix any
golden implementation bugs or modify the exercise scaffold.
