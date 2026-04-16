# 176 Decide Post Release Verify Install Follow-up Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/release_command.rs` shell is now
honest enough after `175`, or whether one more bounded `effigy-release`
extraction batch is still justified before shifting to another `/src` seam.

## In Scope

- inspect the remaining release shell after `175`
- classify what is still reusable release-domain logic versus runner adapter
  work
- decide whether another release extraction batch is warranted
- update lane state and next-task surfaces honestly

## Out Of Scope

- implementation work beyond the decision itself
- release execution
- unrelated cleanup outside the active modularization lane

## Acceptance Criteria

- the remaining release shell is described concretely
- the next move is explicit:
  - either one more ready `effigy-release` extraction batch
  - or the next `/src` modularization seam
- docs currentness reflects the real state

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Decision

Keep the release seam open.

After `175`, the remaining `src/runner/release_command.rs` shell is still not
just adapter work. One more bounded `effigy-release` extraction batch is still
justified before shifting to another `/src` seam.

The remaining reusable layer is:

- git-facing execute helpers:
  - modified-file inspection
  - branch/head/remote checks
  - tag existence checks
  - add/commit/tag/push helpers
- execute-path orchestration around those checks

That is still release-domain API, not just text rendering or final command
dispatch.

## Next Task

Execute
[`177-implement-effigy-release-git-execute-follow-up-extraction.md`](./177-implement-effigy-release-git-execute-follow-up-extraction.md).
