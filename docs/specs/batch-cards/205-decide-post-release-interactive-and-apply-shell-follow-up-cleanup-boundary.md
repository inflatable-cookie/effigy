# 205 Decide Post Release Interactive And Apply Shell Follow Up Cleanup Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining shell in `src/runner/release_command.rs` is now
honest enough to pause after the release interactive/apply cleanup batch, or
whether one last bounded release follow-up is still justified.

## In Scope

- assess what still remains in `src/runner/release_command.rs` after `204`
- decide whether the remainder is now mostly runner-shell orchestration
- record the release boundary honestly in the lane surfaces
- leave one explicit next move:
  - either pause the release seam
  - or open one last bounded release follow-up card

## Out Of Scope

- release execution
- switching to another `/src` seam before the release shell is classified
- container-lane work from the parallel thread

## Acceptance Criteria

- the remaining release runner shell is described concretely
- the next move is explicit and trustworthy
- `continue` resolves through this boundary decision instead of stale `204`
  pointers

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`206-decide-next-src-shell-cleanup-priority-after-release-pause-boundary.md`](./206-decide-next-src-shell-cleanup-priority-after-release-pause-boundary.md)
to choose the next honest `/src` seam now that the release shell is paused.
