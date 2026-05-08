# 196 Implement Effigy Demo Runner Shell Follow Up Cleanup

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Reduce the next largest mixed-responsibility `/src` shell by cleaning up
`src/runner/demo_command.rs` after the distribution boundary pause.

## In Scope

- target the remaining demo runner shell around:
  - render/projection output
  - command bridge flow
  - raw runtime/supervisor wiring
- reduce `src/runner/demo_command.rs` materially again
- keep work bounded to demo runner shell cleanup rather than reopening the full
  demo domain
- update lane state and currentness surfaces honestly

## Out Of Scope

- release closure
- container-lane design work
- reopening distribution
- broad TUI browser cleanup outside the demo runner shell

## Acceptance Criteria

- `src/runner/demo_command.rs` no longer bundles the bulk of the chosen shell
  cluster inline
- the remaining demo runner shell is described honestly after the batch
- the next move is a boundary decision, not another guessed slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`197-decide-post-demo-runner-shell-follow-up-cleanup-boundary.md`](./197-decide-post-demo-runner-shell-follow-up-cleanup-boundary.md)
to decide whether the remaining demo runner shell is honest enough to pause
again.
