# 174 Decide Post Demo Managed Runtime And Backend Follow-up Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/demo_command.rs` shell is now honest
enough after `173`, or whether one more bounded `effigy-demo` extraction batch
is still justified before shifting to another `/src` seam.

## In Scope

- inspect the remaining demo runner shell after `173`
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
  - either one more ready `effigy-demo` extraction batch
  - or a shift to the next `/src` modularization seam
- docs currentness reflects the real state

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

Pause the demo runner seam.

After `173`, the remaining `src/runner/demo_command.rs` shell is now mostly:

- command entry and render wiring
- task/run dispatch orchestration
- raw process launch and supervisor integration
- final runner adapter behavior

That is no longer the next best `effigy-demo` extraction target.

The next real `/src` pressure point is now `src/runner/release_command.rs`.
At `5581` lines, it is materially larger than the remaining demo shell and is
still the biggest unresolved runner surface in the repo.

## Next Task

Execute
[`175-implement-effigy-release-git-and-verify-install-follow-up-extraction.md`](./175-implement-effigy-release-git-and-verify-install-follow-up-extraction.md).
