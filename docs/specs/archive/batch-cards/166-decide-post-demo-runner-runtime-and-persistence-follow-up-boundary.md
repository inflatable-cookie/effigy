# 166 Decide Post Demo Runner Runtime And Persistence Follow-up Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/demo_command.rs` shell is now honest
enough after `165`, or whether one more bounded `effigy-demo` extraction batch
is still justified before shifting to another `/src` seam.

## In Scope

- inspect the remaining demo runner shell after `165`
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

One more bounded `effigy-demo` extraction batch is still justified.

The active-state slice from `165` removed a real persistence cluster, but the
remaining `src/runner/demo_command.rs` shell is still not honest adapter work.
It still owns a reusable demo-domain projection/model layer:

- `DemoRecord`
- `DemoActionAvailability`
- `DemoGroup`
- list/history/projection shaping helpers
- query/group/result projection helpers used by multiple demo command paths

That is still demo-domain API, not just runner text rendering or process
orchestration.

## Next Task

Execute
`167-implement-effigy-demo-record-and-projection-follow-up-extraction.md`
to move the next demo-domain record/projection slice out of
`src/runner/demo_command.rs`.
