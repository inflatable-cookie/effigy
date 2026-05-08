# 201 Decide Post Release Runner Shell Follow Up Cleanup Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the remaining shell in `src/runner/release_command.rs` is now
honest enough to pause after the release runner shell cleanup batch, or whether
one more broader `/src` priority decision should take over.

## In Scope

- assess what still remains in `src/runner/release_command.rs`
- decide whether that remainder is now mostly runner-shell orchestration
- record the release boundary honestly in the lane surfaces
- leave one explicit next move:
  - either pause the release seam
  - or choose the next remaining `/src` cleanup priority

## Out Of Scope

- release execution
- reopening container-lane work from the parallel thread
- speculative extraction beyond what the remaining code actually justifies

## Acceptance Criteria

- the remaining release runner shell is described concretely
- the next move is explicit and trustworthy
- `continue` resolves through this boundary decision instead of stale `199`
  pointers

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`202-implement-effigy-release-context-and-execute-shell-follow-up-cleanup.md`](./202-implement-effigy-release-context-and-execute-shell-follow-up-cleanup.md)
to extract the next coherent release-domain shell that still sits inline in
`src/runner/release_command.rs`.
