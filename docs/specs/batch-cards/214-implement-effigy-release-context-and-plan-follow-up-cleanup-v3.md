# 214 Implement Effigy Release Context And Plan Follow Up Cleanup V3

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the remaining release context-loading and plan-collection layer out of
`src/runner/release_command.rs` so the release seam gets closer to an honest
interactive runner shell.

## In Scope

- target the still-local release-domain layer around:
  - `ReleaseContext`
  - `load_release_context(...)`
  - `collect_release_status(...)`
  - `collect_release_prepare_plan(...)`
  - `collect_release_simulation(...)`
  - `collect_release_execute_plan(...)`
  - related release context and preflight shaping still sitting inline in the runner
- reduce `src/runner/release_command.rs` materially again
- keep the final interactive prompt loop, terminal IO, and runner-side apply/dispatch shell local

## Out Of Scope

- release execution
- release-closure lane work
- demo/container/docs-thread work
- broad shell cleanup outside the active release seam

## Acceptance Criteria

- the release context/plan collection surface no longer sits inline in
  `src/runner/release_command.rs`
- the remaining release runner shell is mostly interactive prompt flow,
  terminal IO, and final runner-side dispatch
- the next move is a boundary decision, not another guessed release slice

## Validation

- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`215-decide-post-release-context-and-plan-follow-up-cleanup-v3-boundary.md`](./215-decide-post-release-context-and-plan-follow-up-cleanup-v3-boundary.md)
to decide whether the remaining release runner shell is now honest enough to
pause or still needs one more bounded cleanup slice.
