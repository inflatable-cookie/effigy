# 328 Decide Post-Embedded-Runner Foundation Boundary

Status: complete
Updated: 2026-05-01
Roadmap: `g03.011`
Spec: `docs/specs/024-embedded-command-script-and-bootstrap-convergence-strict-lane.md`

## Objective

Decide the next honest widening seam after the first shared embedded-runner
foundation.

## In Scope

- audit what `327` now covers:
  - Rhai `run_effigy_command(...)`
  - run-array builtin replay
  - bootstrap task dispatch
- inspect what still sits outside the shared embedded-runner spine:
  - bootstrap managed-run synthesis
  - any remaining caller-local nested projection rules
  - demo or adjacent internal command replay that still bypasses the spine
- decide whether the next batch should:
  - widen `g03.011` once more
  - or stop and hand off to `g03.012`

## Out Of Scope

- implementing the next widening batch itself
- regression-matrix drift guards
- unrelated execution binding cleanup

## Acceptance Criteria

- the post-`327` gap is explicit
- the lane outcome is explicit enough to either widen cleanly or hand off
  without reopening the shared embedded-runner contract

## Validation

- docs-only: `./target/debug/effigy docs check-paths docs/specs/024-embedded-command-script-and-bootstrap-convergence-strict-lane.md docs/specs/batch-cards/327-implement-shared-embedded-runner-foundation.md docs/specs/batch-cards/328-decide-post-embedded-runner-foundation-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/011-embedded-command-script-and-bootstrap-convergence.md`

## Next Task

Promote `g03.012`.
