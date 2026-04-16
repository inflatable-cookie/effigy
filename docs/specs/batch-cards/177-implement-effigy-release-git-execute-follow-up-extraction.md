# 177 Implement Effigy Release Git Execute Follow-up Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next meaningful release-domain shell slice from
`src/runner/release_command.rs` so the git-facing execute cluster stops living
entirely in the runner.

## In Scope

- move one bounded git-facing execute cluster into `effigy-release`
- move branch/head/remote checks and related git helpers that still belong to
  the release domain
- reconnect runner code as a thinner adapter over the extracted API
- update lane state and currentness surfaces honestly

## Out Of Scope

- release execution
- unrelated cleanup outside the release seam
- full `release_command.rs` decomposition in one batch

## Acceptance Criteria

- `effigy-release` widens around one more real release-domain surface
- `src/runner/release_command.rs` shrinks or thins again in a meaningful way
- the remaining release shell is described honestly after the batch

## Validation

- `cargo test -p effigy-release`
- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute `178-decide-post-release-git-execute-follow-up-boundary.md`.
