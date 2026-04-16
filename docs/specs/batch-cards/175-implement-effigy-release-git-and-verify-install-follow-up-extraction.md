# 175 Implement Effigy Release Git And Verify Install Follow-up Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next meaningful release-domain shell slice from
`src/runner/release_command.rs` so the largest remaining runner file stops
owning as much reusable release logic directly.

## In Scope

- identify one bounded release cluster inside `src/runner/release_command.rs`
- move git-facing execution or verify-install orchestration that still belongs
  to `effigy-release`
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

## Outcome

This batch is now shipped.

`effigy-release` owns the verify-install execution cluster:

- tag resolution
- repo-url normalization
- temp fixture setup
- verification-step execution
- install verification orchestration

`src/runner/release_command.rs` is smaller again and now keeps only the
runner-facing repo/remote discovery wrapper around that path.

## Next Task

Execute
[`176-decide-post-release-verify-install-follow-up-boundary.md`](./176-decide-post-release-verify-install-follow-up-boundary.md).
