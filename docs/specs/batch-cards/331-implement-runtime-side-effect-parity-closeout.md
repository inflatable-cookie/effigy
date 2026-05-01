# 331 Implement Runtime Side-Effect Parity Closeout

Status: complete
Updated: 2026-05-01
Roadmap: `g03.012`
Spec: `docs/specs/025-regression-matrix-and-drift-guards-strict-lane.md`

## Objective

Close the remaining high-signal `g03.012` parity gaps around runtime side
effects rather than command parsing or ownership classification.

## In Scope

- add one bounded proof slice for bootstrap `start` / workspace-handoff parity
- add one bounded proof slice for gateway and alias reconciliation parity on
  shared runtime prep paths
- add one bounded proof slice for host-container lease refresh parity across
  explicit task, deferred task, and `exec` activation

## Out Of Scope

- new convergence refactors unless these proofs expose an immediate break
- widening into a broad end-to-end shell or browser suite
- reopening embedded repo-targeting or inline workspace error-family work

## Acceptance Criteria

- the remaining runtime side effects called out in `g03.012` are proven by
  focused tests
- any still-deliberate exception stays explicit and tested
- the lane is then in a credible position to pause or close

## Validation

- targeted runtime-side-effect parity tests
- `./target/debug/effigy docs check-paths docs/specs/025-regression-matrix-and-drift-guards-strict-lane.md docs/specs/batch-cards/330-decide-post-parity-matrix-foundation-boundary.md docs/specs/batch-cards/331-implement-runtime-side-effect-parity-closeout.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/012-regression-matrix-and-drift-guards.md`

## Next Task

No active next task. This closeout slice is complete.
