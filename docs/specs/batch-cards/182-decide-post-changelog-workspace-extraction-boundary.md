# 182 Decide Post Changelog Workspace Extraction Boundary

Status: ready
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining `src/runner/release_command.rs` shell is now
honest enough after changelog extraction or still contains one more bounded
workspace-worthy seam before `g02.010` can pause for release closure.

## In Scope

- assess the remaining release shell after `181`
- classify what is still reusable release-domain API versus shell/runtime glue
- update lane state and currentness surfaces honestly

## Out Of Scope

- another implementation extraction unless the decision proves it is needed
- release closure
- unrelated cleanup outside the remaining release seam

## Acceptance Criteria

- the remaining release shell is described concretely
- the lane either pauses the release seam or opens one explicit next card
- currentness surfaces stop advertising `181`

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute this boundary decision, then either pause `g02.010` for release
resumption or open one final bounded release-shell extraction card.
