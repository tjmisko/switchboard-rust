# Documentation

Two audiences share this directory, and the split is the whole point of the
layout.

**For the learner working the course** (`exercises/README.md` has the reading
order):

| File | Read before | What it settles |
|---|---|---|
| `02-session-machine.md` | T07 | phase as a fold over the axes, the turn's tool map, one machine per child |
| `03-types-and-transitions.md` | T03 | why `src/model.rs` has the shape it has, and which harness signal drives each transition |
| `04-cost-model.md` | T09 | every latency and hysteresis constant, and the asymmetry behind `Shown` |
| `architecture-explorer/` | any time | an interactive picture of the pipeline; open `index.html` |

**For the owner running the product track**: everything under `product/`.
Plan, concurrency argument, surveys of the Go daemon and the harnesses,
audits, handoffs and open items. Those documents change without notice and
the course does not depend on them. A task sheet that needs a fact from one
of them says so and names the section.
