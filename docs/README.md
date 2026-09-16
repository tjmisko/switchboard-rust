# Documentation

Two audiences share this directory, and the split is the whole point of the
layout.

**For the learner working the course** (`exercises/README.md` has the reading
order):

| File | Read before | What it settles |
|---|---|---|
| `02-session-machine.md` | T08, optional | the argument for phase as a fold, the turn's tool map, one machine per child; the conclusions are restated in `src/reduce/mod.rs` in the code's own vocabulary |
| `03-types-and-transitions.md` | T08, optional | why `src/model.rs` has the shape it has; its transition sections (§4, §13) are reducer material, not adapter material |
| `04-cost-model.md` | T10 | every latency and hysteresis constant, and the asymmetry behind `Shown` |
| `architecture-explorer/` | any time | an interactive picture of the pipeline; open `index.html` |

**For the owner running the product track**: everything under `product/`.
Plan, concurrency argument, surveys of the Go daemon and the harnesses,
audits, handoffs and open items. Those documents change without notice and
the course does not depend on them. A task sheet that needs a fact from one
of them says so and names the section.
