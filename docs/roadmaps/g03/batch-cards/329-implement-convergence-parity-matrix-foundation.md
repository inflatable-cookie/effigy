# 329 Implement Convergence Parity Matrix Foundation

Status: archived
Updated: 2026-05-01
Roadmap: `g03.012`
Spec: `docs/specs/025-regression-matrix-and-drift-guards-strict-lane.md`

## Objective

Add the first bounded parity matrix and drift guards for the converged
execution surfaces.

## In Scope

- add focused fixture coverage for the highest-signal parity scenarios:
  - stopped runtime plus explicit container task
  - stopped runtime plus deferred container request
  - stopped runtime plus `effigy exec`
  - direct workspace shell exit versus seeded task shell exit
  - run-array builtin repo targeting
  - Rhai repo targeting
- add one explicit guard against new duplicated embedded repo-targeting match
  blocks
- add one explicit guard for intentional unsupported-surface error-family
  parity where the contract already calls that out

## Out Of Scope

- giant end-to-end smoke suites
- new convergence refactors unless the added tests expose one immediately
- widening the fixture matrix beyond the contract’s minimum proof set

## Acceptance Criteria

- the first convergence parity matrix exists and runs
- shared embedded repo targeting has an explicit drift guard
- at least one intentional exception family is locked down by test instead of
  by docs alone

## Validation

- targeted parity/drift-guard tests
- `./target/debug/effigy docs check-paths docs/specs/025-regression-matrix-and-drift-guards-strict-lane.md docs/roadmaps/g03/batch-cards/329-implement-convergence-parity-matrix-foundation.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/012-regression-matrix-and-drift-guards.md`

## Next Task

Execute `330`.
