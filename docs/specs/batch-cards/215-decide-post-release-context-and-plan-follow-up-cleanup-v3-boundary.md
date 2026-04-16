# 215 Decide Post Release Context And Plan Follow Up Cleanup V3 Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining shell in `src/runner/release_command.rs` is now
honest enough to pause after `214`, or whether one more bounded release
follow-up is still justified.

## In Scope

- assess what still remains in `src/runner/release_command.rs` after `214`
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
- `continue` resolves through this boundary decision instead of stale `214`
  pointers

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`216-implement-effigy-release-apply-and-gate-follow-up-cleanup-v4.md`](./216-implement-effigy-release-apply-and-gate-follow-up-cleanup-v4.md)
to move the remaining release apply/gate execution layer out of
`src/runner/release_command.rs`.
