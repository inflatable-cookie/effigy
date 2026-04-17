# 234 Decide Next Src Shell Cleanup Priority After Effigy Process Pause Boundary

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`, `g02.017`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next meaningful `/src` seam to reduce after pausing process
supervision, using the `g02.017` queue as the source of truth for remaining
disjoint jobs.

## In Scope

- survey the remaining `g02.017` queue jobs that are not under parallel-thread
  churn and not already done
- choose the next best cleanup priority for `g02.010`
- promote the decision into the active lane/currentness surfaces

## Out Of Scope

- release execution
- demo/docs/container parallel cleanup (those seams are under parallel-thread
  write-set)

## Acceptance Criteria

- the next `/src` priority after process supervision is explicit
- the reason for that priority is recorded
- the active lane/currentness surfaces point at the next ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`235-implement-effigy-ui-subsystem-extraction.md`](./235-implement-effigy-ui-subsystem-extraction.md)
to move `src/ui/**` into a new `effigy-ui` crate per `g02.017` queue job #6.
