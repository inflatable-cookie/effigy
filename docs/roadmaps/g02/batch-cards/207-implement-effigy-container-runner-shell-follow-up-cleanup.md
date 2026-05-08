# 207 Implement Effigy Container Runner Shell Follow Up Cleanup

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Reduce the next largest still-live `/src` shell by cleaning up
`src/runner/container_command.rs` now that the release seam is paused on an
honest runner-shell boundary.

## In Scope

- target the remaining container runner shell around:
  - container session orchestration
  - attach/stream/TUI flow shaping
  - Colima and compose execution helpers that still belong in
    `effigy-containers`
- reduce `src/runner/container_command.rs` materially again
- keep work bounded to runner-shell cleanup rather than reopening container
  contract/design planning
- update lane state and currentness surfaces honestly

## Out Of Scope

- container-design roadmap work from the parallel thread
- release-lane execution
- unrelated cleanup outside the active modularization lane

## Acceptance Criteria

- `src/runner/container_command.rs` no longer bundles the bulk of the chosen
  container runner shell cluster inline
- the remaining container shell is described honestly after the batch
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo fmt --all`
- `cargo test -p effigy-containers`
- `cargo test --test cli_output_tests container`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`208-decide-post-container-runner-shell-follow-up-cleanup-boundary.md`](./208-decide-post-container-runner-shell-follow-up-cleanup-boundary.md)
to decide whether the remaining container runner shell is honest enough to
pause or whether one more bounded container follow-up is justified.
