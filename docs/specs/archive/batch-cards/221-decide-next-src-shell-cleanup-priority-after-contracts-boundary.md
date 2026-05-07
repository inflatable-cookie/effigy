# 221 Decide Next Src Shell Cleanup Priority After Contracts Boundary

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next meaningful `/src` seam to reduce after pausing the contracts
boundary.

## In Scope

- assess the remaining heavy `/src` shells after `220`
- choose the next best cleanup priority for `g02.010`
- prefer a substantial seam that does not create avoidable write-set conflict
- promote the decision into the active lane/currentness surfaces

## Out Of Scope

- release execution
- speculative new crate work without a real shell seam
- broad roadmap churn outside the active lane

## Acceptance Criteria

- the next `/src` priority after contracts is explicit
- the reason for that priority is recorded
- the active lane/currentness surfaces point at the next ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`222-implement-effigy-distribution-runner-shell-follow-up-cleanup-v2.md`](./222-implement-effigy-distribution-runner-shell-follow-up-cleanup-v2.md)
to reduce the next meaningful distribution runner shell slice.
