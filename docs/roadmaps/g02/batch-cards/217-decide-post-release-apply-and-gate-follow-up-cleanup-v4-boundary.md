# 217 Decide Post Release Apply And Gate Follow Up Cleanup V4 Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining shell in `src/runner/release_command.rs` is now
honest enough to pause after `216`, or whether one more bounded release
follow-up is still justified.

## In Scope

- assess what still remains in `src/runner/release_command.rs` after `216`
- decide whether the remainder is now mostly interactive runner-shell work
- record the release boundary honestly in the lane surfaces
- leave one explicit next move:
  - either pause the release seam
  - or open one more bounded release follow-up card

## Out Of Scope

- release execution
- switching to another `/src` seam before the release shell is classified
- demo/container/docs-thread work

## Acceptance Criteria

- the remaining release runner shell is described concretely
- the next move is explicit and trustworthy
- `continue` resolves through this boundary decision instead of stale `216`
  pointers

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`218-decide-next-src-shell-cleanup-priority-after-release-final-pause-boundary.md`](./218-decide-next-src-shell-cleanup-priority-after-release-final-pause-boundary.md)
to choose the next honest `/src` seam now that the release shell is paused.
