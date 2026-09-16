# Product notes

The owner's working documents for the rewrite. Nothing here is required
reading for the course in `exercises/`; the learner-facing documents stay
one level up in `docs/`.

| # | File | What it is |
|---|---|---|
| 00 | `00-rewrite-plan.md` | the synthesis of the surveys and the plan for the rewrite |
| 01 | `01-concurrency-design.md` | the argument for one core actor and pure reducers |
| 05 | `05-modules.md` | the two axes of churn, platforms and harnesses, and the module layout that isolates each |
| 06 | `06-dashboard-boundary.md` | what the daemon owes the dashboard |
| 07 | `07-history-data-model.md` | the history record's own shape |
| 08 | `08-dashboard-redesign.md` | write/anneal/read for the dashboard |
| 09 | `09-precomputation-boundary.md` | where precomputation stops; supersedes 08 §4 |
| 10 | `10-open-items.md` | what is undecided or unrecorded, ordered by how much it blocks |
| 11 | `11-golden-audit-handoff.md` | the golden audit and the redesign-planning handoff (golden only) |
| 13 | `13-capture-fix-handoff.md` | the September 2026 capture fix queue and its status (golden only) |
| 14 | `14-dryrun-findings.md` | the 2026-09-16 dry run of the course by three learner agents: measured ramp, findings, tiered fix plan; journals in `dryrun/`, per-range scaffold porting notes in `scaffold-notes/` |
| | `survey/` | read-only investigations of the Go daemon, the prior Rust attempt and the harness binaries |
| | `audit/` | parity, complexity, adversarial and coverage audits of golden, with repros (golden only) |

Numbers 02, 03 and 04 are learner documents and live in `docs/`. Number 12 is
reserved for the redesign decisions.
