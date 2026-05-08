# 195 Decide Next Src Shell Cleanup Priority After Distribution Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next honest `/src` seam after the distribution boundary now that
`effigy-distribution` is paused on an adapter shell.

## In Scope

- assess the remaining large `/src` shells
- pick the next bounded cleanup target
- record the decision honestly in the lane surfaces
- open the next ready card for that seam

## Out Of Scope

- release closure
- reopening distribution without a concrete new reason
- container-lane design work

## Acceptance Criteria

- the next `/src` priority after distribution is explicit
- the lane does not advertise distribution as still active work
- one ready card is opened for the chosen next seam

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`196-implement-effigy-demo-runner-shell-follow-up-cleanup.md`](./196-implement-effigy-demo-runner-shell-follow-up-cleanup.md)
to reduce the next largest mixed-responsibility runner shell after the
distribution boundary.
