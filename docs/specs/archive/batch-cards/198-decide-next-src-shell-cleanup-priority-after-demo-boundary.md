# 198 Decide Next Src Shell Cleanup Priority After Demo Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next honest `/src` shell-cleanup priority now that the demo runner
seam is paused on an adapter/process boundary.

## In Scope

- compare the remaining large `/src` seams after the demo boundary decision
- choose the next cleanup target based on real modularization value, not line
  count alone
- update lane state and currentness surfaces honestly
- open one explicit ready card for the chosen seam

## Out Of Scope

- implementation work beyond the decision itself
- release-lane execution
- container-lane design work that belongs to the parallel thread

## Acceptance Criteria

- the next `/src` priority is explicit
- the reason for that choice is recorded concretely
- the next move is one ready implementation card, not an ambiguous list

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`199-implement-effigy-release-runner-shell-follow-up-cleanup.md`](./199-implement-effigy-release-runner-shell-follow-up-cleanup.md)
to reduce the next bounded release runner shell slice.
