# 216 Implement Effigy Release Apply And Gate Follow Up Cleanup V4

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the remaining release apply/gate execution layer out of
`src/runner/release_command.rs` so the release seam gets closer to an honest
interactive runner shell.

## In Scope

- target the still-local release-domain layer around:
  - `execute_release_prepare(...)`
  - `execute_release(...)`
  - `run_release_gates(...)`
  - standalone gate-run shaping that still sits inline in the runner
- reduce `src/runner/release_command.rs` materially again
- keep the final interactive prompt loop, terminal IO, and runner-side dispatch
  shell local

## Out Of Scope

- release execution itself as a user workflow
- switching to another `/src` seam before the release shell is classified
- demo/container/docs-thread work

## Acceptance Criteria

- the release apply/gate execution surface no longer sits inline in
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
[`217-decide-post-release-apply-and-gate-follow-up-cleanup-v4-boundary.md`](./217-decide-post-release-apply-and-gate-follow-up-cleanup-v4-boundary.md)
to decide whether the remaining release runner shell is now honest enough to
pause or still needs one more bounded cleanup slice.
