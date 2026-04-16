# 209 Decide Next Src Shell Cleanup Priority After Container Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next honest `/src` shell cleanup priority now that the container
runner seam can pause on an adapter/process boundary.

## In Scope

- compare the remaining large `/src` seams after the container boundary
- account for the still-active parallel docs thread so this lane avoids
  avoidable write-set conflict
- choose the next cleanup target based on real modularization value, not line
  count alone
- update lane state and currentness surfaces honestly
- open one explicit ready next move

## Out Of Scope

- implementation work beyond the decision itself
- release-lane execution
- new container-design work

## Acceptance Criteria

- the next `/src` priority is explicit
- the reason for that choice is recorded concretely
- the next move is one ready card, not an ambiguous list

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`210-implement-effigy-release-runner-shell-follow-up-cleanup-v2.md`](./210-implement-effigy-release-runner-shell-follow-up-cleanup-v2.md)
to reduce the next bounded release runner shell slice.
