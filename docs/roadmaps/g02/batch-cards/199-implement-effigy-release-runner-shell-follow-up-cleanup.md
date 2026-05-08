# 199 Implement Effigy Release Runner Shell Follow Up Cleanup

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Reduce the next largest mixed-responsibility `/src` shell by cleaning up
`src/runner/release_command.rs` after the demo runner boundary pause.

## In Scope

- target the remaining release runner shell around:
  - interactive review and prompt flow
  - text/status rendering helpers
  - runner-side prepare/execute/resume shell coordination
- reduce `src/runner/release_command.rs` materially again
- keep work bounded to runner-shell cleanup rather than reopening broad release
  domain extraction by default
- update lane state and currentness surfaces honestly

## Out Of Scope

- release execution
- container-lane design work
- unrelated cleanup outside the active modularization lane

## Acceptance Criteria

- `src/runner/release_command.rs` no longer bundles the bulk of the chosen
  release shell cluster inline
- the remaining release shell is described honestly after the batch
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`201-decide-post-release-runner-shell-follow-up-cleanup-boundary.md`](./201-decide-post-release-runner-shell-follow-up-cleanup-boundary.md)
to decide whether the remaining release runner shell is honest enough to pause
again or whether one more broader `/src` priority decision is now justified.
