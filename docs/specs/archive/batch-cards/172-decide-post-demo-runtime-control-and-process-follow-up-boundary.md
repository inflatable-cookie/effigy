# 172 Decide Post Demo Runtime Control And Process Follow-up Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/demo_command.rs` shell is now honest
enough after `171`, or whether one more bounded `effigy-demo` extraction batch
is still justified before shifting to another `/src` seam.

## In Scope

- inspect the remaining demo runner shell after `171`
- classify what is still reusable demo-domain logic versus runner adapter work
- decide whether another demo extraction batch is warranted
- update lane state and next-task surfaces honestly

## Out Of Scope

- implementation work beyond the decision itself
- release-lane execution
- unrelated runner cleanup outside the active modularization lane

## Acceptance Criteria

- the remaining demo runner shell is described concretely
- the next move is explicit:
  - either one more ready `effigy-demo` extraction card
  - or a shift to the next `/src` seam
- docs currentness reflects the real state

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

Keep the demo runner seam open.

After `171`, the remaining `src/runner/demo_command.rs` shell is still not
just adapter work. One more bounded `effigy-demo` extraction batch is still
justified before shifting to another `/src` seam.

The remaining reusable layer is:

- managed runtime state and event-loop handling
- runtime backend classification and projection helpers
- stop/attach capability shaping around concurrent-runner demos

Those pieces still look like crate-owned demo runtime API, not terminal-only
shell glue.

## Next Task

Execute
[`173-implement-effigy-demo-managed-runtime-and-backend-follow-up-extraction.md`](./173-implement-effigy-demo-managed-runtime-and-backend-follow-up-extraction.md).
