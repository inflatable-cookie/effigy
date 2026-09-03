# Cross-Repo Vision Rollout Scorecard — 2026-09-03

Window
- Review Cadence: monthly
- Criteria: [`007` maturity stages](../007-vision-adoption-and-maturity-model-v1.md);
  template [`016`](../016-cross-repo-rollout-comparison-scorecard-template-v1.md)
- Evidence window: the frozen Acowtancy replay of card
  [`1111`](../../roadmaps/g09/batch-cards/1111-acowtancy-consumer-adoption-replay.md)
  at consumer SHA `91228893cbc2c6440b115b5aa1ee2fe34064f35b` with Effigy
  `e44da9fd59e4696d4c7868d6c7e528201eb41e24`; command evidence is in log
  [`03-014518`](../../logs/2026-09/03-014518-acowtancy-consumer-replay-1111.md)
  ("the evidence log").

Comparison Table

| Repo | Overall Stage | ROUTE | CONTRACT | OPERATE | MAINT | RELEASE | Active Risks | Active Exceptions | Overdue Exceptions | Recent Movement |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Effigy | 2 | 2 | 2 | 2 | 2 | 2 | 5 | 0 | 0 | up |
| Acowtancy | unknown | unknown | unknown | unknown | unknown | unknown | unknown | unknown | unknown | unknown |

- Effigy overall 2 — [`019` baseline](../019-effigy-vision-maturity-baseline-v1.md)
  ("Stage 2 maintained", reviewed 2026-09-03); the per-dimension scores below
  stay at 2 because no in-window evidence moved any dimension.
- Effigy ROUTE 2 — deterministic routing is test-covered and monitored:
  suite-selection/routing tests exist (evidence log
  [`01-173500`](../../logs/2026-09/01-173500-child-catalog-suite-registry-1100.md)
  validation rows), and in-window orientation `effigy tasks` / `effigy doctor`
  exited 0 with `qa:ci` on the task surface.
- Effigy CONTRACT 2 — enforced validation bundle (`qa` composing docs QA and
  JSON contracts; `effigy qa:docs` passed in this window); JSON contract
  surface documented in guide `017`.
- Effigy OPERATE 2 — remediation-first doctor output observed in-window
  (findings carry remediation text and auto-fix flags); doctor explain
  provides non-executing routing evidence.
- Effigy MAINT 2 — doctor scan integration with remediation surfaced the
  pre-existing `god-files` warning in-window; papercuts capture and log
  retention conventions are active.
- Effigy RELEASE 2 — release gates and orchestration are first-class
  (guide `051`; tagged `v0.12.1` releases); no bypass pressure in-window.
- Acowtancy — no stage scores. `007` advancement rule 1 requires all prior
  stage criteria, and the clean replay deliberately collected no full-doctor,
  JSON-contract, or release-gate evidence, so no Acowtancy aggregate or
  dimension reaches stage 1 on this window. Observed facts from the clean
  matrix, recorded without stage claims:
  - doctor explain resolved `docs/qa:docs` to the `docs` catalog via explicit
    prefix (`selection-status: ok`), nine catalogs inventoried, seven-member
    test plan resolved deterministically (evidence log, matrix rows 1–3).
  - `docs/qa:docs` passed 5/5 and `docs/qa:northstar` 6/6 (matrix rows 4–5).
  - Integrated health is **unavailable** under the replay's read-only
    boundary (full doctor executes repo-owned health tasks); unavailability
    is not scored as passing or failing.
  - Docs-spine maintenance surfaces are active (generation index, triage,
    status board, guards); consumer scan posture is unknown in the clean
    window because the only scan observation came from the excluded discovery
    run. No release surface was in replay scope (the workspace-container
    exception puts no changelog/release expectation on the orchestration
    root). Risks, exceptions, and movement are unmeasured: the replay did not
    read the consumer's governance register, and its product activity
    (cards `193`–`197`) is not a maturity measurement.

Priority Actions
- Repo: Acowtancy
- Constraint: integrated health evidence and the retained workspace-root
  re-entry workaround both await Acowtancy-owned revalidation of Effigy's
  child-catalog suite registry fix.
- Action: revalidate `farmyard/health` (failed at the `6bcf6c70…` discovery
  run) and the nested child-catalog invocation path; then keep or retire the
  root re-entry workaround by Acowtancy's own decision.
- Owner: Acowtancy maintainers
- Due: next cohort window

Interpretation Notes

- One pilot is one pilot: this table makes no universal consumer-compatibility
  claim. Cohort expansion is a separate decision (`D-2026-05`).
- No Acowtancy stage is asserted: the window supports observations, not a
  `007` stage claim, because full doctor, JSON-contract, and release-gate
  criteria were intentionally not exercised. A future window that can observe
  those criteria without violating the read-only consumer boundary is required
  before scoring Acowtancy.
- Unknown cells stay unknown until such a window exists; they are not zeros
  and not passing marks.
