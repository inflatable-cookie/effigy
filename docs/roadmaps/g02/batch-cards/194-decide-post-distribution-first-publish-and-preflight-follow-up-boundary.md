# 194 Decide Post Distribution First Publish And Preflight Follow Up Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining distribution shell in
`src/runner/distribution_command.rs` is now honest enough to pause after the
first-publish and preflight follow-up extraction.

## In Scope

- inspect what still remains in `distribution_command.rs`
- decide whether the remaining shell is now mostly adapter/orchestration work
- record the decision honestly in the lane surfaces
- set the next ready card only if another bounded distribution slice is still
  justified

## Out Of Scope

- release closure
- container-lane design work
- shifting to another seam without recording the distribution boundary first

## Acceptance Criteria

- the post-`193` distribution boundary is recorded clearly
- the next move is explicit:
  - either distribution pauses cleanly
  - or one more bounded distribution card is opened

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`195-decide-next-src-shell-cleanup-priority-after-distribution-boundary.md`](./195-decide-next-src-shell-cleanup-priority-after-distribution-boundary.md)
to choose the next remaining `/src` seam now that distribution can pause.
