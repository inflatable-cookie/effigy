# 197 Decide Post Demo Runner Shell Follow Up Cleanup Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/demo_command.rs` shell is now honest
enough after `196`, or whether one more bounded demo cleanup batch is still
justified before shifting to the next `/src` seam.

## In Scope

- inspect what still remains in `src/runner/demo_command.rs` after `196`
- classify the remaining demo runner shell as either:
  - reusable demo-domain logic
  - runner adapter / process-shell work
- update lane state and currentness surfaces honestly
- set the next ready card only if another bounded demo slice is still justified

## Out Of Scope

- implementation work beyond the decision itself
- release-lane execution
- shifting to another seam without recording the demo boundary first

## Acceptance Criteria

- the post-`196` demo runner boundary is recorded clearly
- the next move is explicit:
  - either the demo runner seam pauses cleanly
  - or one more bounded demo cleanup card is opened

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`198-decide-next-src-shell-cleanup-priority-after-demo-boundary.md`](./198-decide-next-src-shell-cleanup-priority-after-demo-boundary.md)
to choose the next remaining `/src` seam now that the demo runner shell can
pause.
