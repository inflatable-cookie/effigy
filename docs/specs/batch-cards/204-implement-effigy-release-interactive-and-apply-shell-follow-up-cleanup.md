# 204 Implement Effigy Release Interactive And Apply Shell Follow Up Cleanup

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the remaining bounded release-domain shell still sitting inline in
`src/runner/release_command.rs` now that context loading and plan collection
are crate-owned.

## In Scope

- widen `effigy-release` around the next remaining reusable release shell:
  - interactive review/apply flow where it is still release-domain rather than
    raw terminal IO
  - release prepare/apply orchestration
  - release execute/apply orchestration
  - release-specific progress/error adaptation where it is still domain-owned
- reduce `src/runner/release_command.rs` materially again
- keep the batch bounded to release shell cleanup rather than reopening the
  release lane itself
- update lane state and currentness surfaces honestly

## Out Of Scope

- release execution by a human
- shifting to another `/src` seam before the release shell is classified again
- container-lane work from the parallel thread

## Acceptance Criteria

- `src/runner/release_command.rs` no longer owns the bulk of the chosen
  interactive/apply release shell cluster
- the remaining release shell is described concretely after the batch
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`205-decide-post-release-interactive-and-apply-shell-follow-up-cleanup-boundary.md`](./205-decide-post-release-interactive-and-apply-shell-follow-up-cleanup-boundary.md)
to decide whether the remaining release runner shell is finally honest enough
to pause or still needs one last bounded follow-up.
