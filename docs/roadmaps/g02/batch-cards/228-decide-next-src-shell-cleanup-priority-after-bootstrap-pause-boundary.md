# 228 Decide Next Src Shell Cleanup Priority After Bootstrap Pause Boundary

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next meaningful `/src` seam to reduce after pausing bootstrap, or
record that the strict lane has reached an honest full pause because the
remaining disjoint seams are either already paused or under parallel-thread
churn.

## In Scope

- survey the remaining `src/runner/*.rs` shells after `227`
- classify each as: paused, under parallel-thread churn, or eligible
- choose the next best cleanup priority for `g02.010`, or pause the lane
- promote the decision into the active lane/currentness surfaces

## Out Of Scope

- release execution
- speculative new crate work without a real shell seam
- stepping on seams that are under parallel-thread write-set

## Acceptance Criteria

- the next `/src` priority after bootstrap is explicit
- the reason for that priority (or for pausing the lane) is recorded
- the active lane/currentness surfaces point at the next ready card, or mark
  the strict lane as paused on an honest boundary

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`229-implement-effigy-cli-help-extraction.md`](./229-implement-effigy-cli-help-extraction.md)
to move the root-owned CLI help surface into `effigy-cli`.
