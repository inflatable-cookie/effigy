# 168 Decide Post Demo Record And Projection Follow-up Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/demo_command.rs` shell is now honest
enough after `167`, or whether one more bounded `effigy-demo` extraction batch
is still justified before shifting to another `/src` seam.

## In Scope

- inspect the remaining demo runner shell after `167`
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

`167` removed the shared record/projection layer cleanly, but
`src/runner/demo_command.rs` still owns a reusable demo execution/runtime
cluster that is larger and more domain-shaped than the next obvious `/src`
seams:

- `DemoExecutionAttempt`
- `DemoLogPaths`
- run-backed launch and output capture helpers
- concurrent-runner runtime state and projection helpers
- receipt/history/log persistence shaping around executed attempts

That is still demo-domain API and lifecycle ownership, not just shell-level
command dispatch or text rendering.

## Next Task

Execute
`169-implement-effigy-demo-execution-runtime-and-attempt-follow-up-extraction.md`
to move the next demo execution/runtime slice out of
`src/runner/demo_command.rs`.
