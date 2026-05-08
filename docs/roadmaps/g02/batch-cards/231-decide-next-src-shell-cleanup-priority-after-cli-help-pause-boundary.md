# 231 Decide Next Src Shell Cleanup Priority After CLI Help Pause Boundary

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next meaningful `/src` seam to reduce after pausing CLI help, using
the `g02.017` queue as the source of truth for remaining disjoint jobs.

## In Scope

- survey the remaining `g02.017` queue jobs that are not under parallel-thread
  churn
- choose the next best cleanup priority for `g02.010`
- promote the decision into the active lane/currentness surfaces

## Out Of Scope

- release execution
- demo/docs/container parallel cleanup (those seams are under parallel-thread
  write-set)
- stepping on seams that are under parallel-thread write-set

## Acceptance Criteria

- the next `/src` priority after CLI help is explicit
- the reason for that priority is recorded
- the active lane/currentness surfaces point at the next ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`232-implement-effigy-process-subsystem-extraction.md`](./232-implement-effigy-process-subsystem-extraction.md)
to move `src/process_manager/**` into a new `effigy-process` crate per
`g02.017` queue job #4.
