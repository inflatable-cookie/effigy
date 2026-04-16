# 188 Decide Next Src Shell Cleanup Priority After Bootstrap Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next honest `/src` shell-cleanup seam now that bootstrap is paused
on an adapter boundary.

## In Scope

- confirm that bootstrap no longer needs another crate extraction batch
- reassess the remaining large `/src` seams
- choose the next bounded modularization move
- update lane state and currentness surfaces honestly

## Out Of Scope

- implementing the next seam in the same batch
- release closure
- reopening already-paused seams without a new justified reason

## Acceptance Criteria

- bootstrap is either explicitly paused or explicitly reopened with a reason
- the next ready card targets one clear `/src` seam
- the lane stays anchored on meaningful cleanup instead of atomized churn

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`189-implement-effigy-distribution-execution-and-artifact-follow-up-extraction.md`](./189-implement-effigy-distribution-execution-and-artifact-follow-up-extraction.md).
