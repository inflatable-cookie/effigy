# 140 Implement Effigy Doctor Report And Projection Extraction

Status: archived
Updated: 2026-04-15
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the reusable doctor report/result cluster into `effigy-doctor` so the
doctor result model, summary logic, and projection-prep contracts no longer
depend entirely on `runner`.

## In Scope

- widen `effigy-doctor` around doctor result/report ownership
- move the next trustworthy doctor report/projection contracts there
- reconnect the current runner path without changing user-facing behavior
- leave the next modularization batch explicit

## Out Of Scope

- broad doctor render/run extraction in the same batch
- release closure
- vault-provider rollout work

## Acceptance Criteria

- more of the doctor domain no longer sits entirely in `runner`
- the doctor report/projection boundary is clearer and more reusable than today
- the next modularization batch is explicit

## Validation

- targeted Rust validation for the moved doctor contracts
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`141-decide-post-doctor-report-and-projection-extraction-boundary.md`](./141-decide-post-doctor-report-and-projection-extraction-boundary.md)
to classify the remaining doctor shell before modularization jumps again.
