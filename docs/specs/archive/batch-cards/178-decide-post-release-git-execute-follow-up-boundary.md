# 178 Decide Post Release Git Execute Follow-up Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/release_command.rs` shell is now
honest adapter work or still contains one more bounded `effigy-release`
extraction seam before `g02.010` can move on.

## In Scope

- assess the remaining release shell after `177`
- classify what is still reusable release-domain API versus shell/runtime glue
- update lane state and currentness surfaces honestly

## Out Of Scope

- another implementation extraction unless the decision proves it is needed
- release closure
- unrelated cleanup outside the release seam

## Acceptance Criteria

- the remaining release shell is described concretely
- the lane either pauses the release seam or opens one explicit next card
- currentness surfaces stop advertising `177`

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `179-implement-effigy-release-version-and-preview-follow-up-extraction.md`.
