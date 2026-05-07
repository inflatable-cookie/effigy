# 202 Implement Effigy Release Context And Execute Shell Follow Up Cleanup

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next coherent release-domain shell still sitting inline in
`src/runner/release_command.rs` now that review/prompt parsing and shell-facing
text rendering are crate-owned.

## In Scope

- widen `effigy-release` around the next remaining reusable release cluster:
  - release context loading
  - prepare/simulate/status plan collection shaping
  - execute orchestration helpers where they are still release-domain rather
    than terminal IO
- reduce `src/runner/release_command.rs` materially again
- keep the batch bounded to release shell cleanup rather than reopening broad
  release-lane execution
- update lane state and currentness surfaces honestly

## Out Of Scope

- release execution itself
- container-lane work from the parallel thread
- shifting to another `/src` seam without finishing the current release
  boundary honestly

## Acceptance Criteria

- `src/runner/release_command.rs` no longer owns the bulk of the chosen
  release context/execute shell cluster
- the remaining release shell is described concretely after the batch
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`203-decide-post-release-context-and-execute-shell-follow-up-cleanup-boundary.md`](./203-decide-post-release-context-and-execute-shell-follow-up-cleanup-boundary.md)
to decide whether the remaining release runner shell is finally honest enough
to pause or still needs one more bounded follow-up.
