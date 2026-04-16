# 170 Decide Post Demo Execution Runtime And Attempt Follow-up Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/demo_command.rs` shell is now honest
enough after `169`, or whether one more bounded `effigy-demo` extraction batch
is still justified before shifting to another `/src` seam.

## In Scope

- inspect the remaining demo runner shell after `169`
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

## Next Task

## Decision

One more bounded `effigy-demo` extraction batch is still justified.

`169` removed the shared attempt/log execution layer cleanly, but
`src/runner/demo_command.rs` still owns a reusable runtime-control cluster that
is larger and more domain-shaped than the next obvious `/src` seams:

- concurrent-runner runtime state and event-loop handling
- run-backed launch mode and PTY/stream process shaping
- output capture / input handoff helpers
- runtime backend classification and projected-process helpers

That is still demo-domain runtime API and lifecycle ownership, not just shell
command routing or text rendering.

## Next Task

Execute
`171-implement-effigy-demo-runtime-control-and-process-follow-up-extraction.md`
to move the next demo runtime-control slice out of
`src/runner/demo_command.rs`.
