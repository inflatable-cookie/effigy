# 203 Decide Post Release Context And Execute Shell Follow Up Cleanup Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining shell in `src/runner/release_command.rs` is now
honest enough to pause after the release context/execute cleanup batch, or
whether one more bounded release shell cleanup batch is still justified.

## In Scope

- assess what still remains in `src/runner/release_command.rs` after `202`
- decide whether the remainder is now mostly runner-shell orchestration
- record the release boundary honestly in the lane surfaces
- leave one explicit next move:
  - either pause the release seam
  - or open one more bounded release follow-up card

## Out Of Scope

- release execution
- switching to another `/src` seam before the release shell is classified
- container-lane work from the parallel thread

## Acceptance Criteria

- the remaining release runner shell is described concretely
- the next move is explicit and trustworthy
- `continue` resolves through this boundary decision instead of stale `202`
  pointers

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`204-implement-effigy-release-interactive-and-apply-shell-follow-up-cleanup.md`](./204-implement-effigy-release-interactive-and-apply-shell-follow-up-cleanup.md)
to extract the remaining interactive review/apply release shell that still
justifies one more bounded cleanup pass.
