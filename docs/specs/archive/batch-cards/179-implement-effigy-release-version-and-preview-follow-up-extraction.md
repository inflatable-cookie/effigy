# 179 Implement Effigy Release Version And Preview Follow-up Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the next meaningful release-domain slice from
`src/runner/release_command.rs` so version-file authoring, changelog mutation
shaping, and mutation preview helpers stop living entirely in the runner.

## In Scope

- move one bounded release version/mutation helper cluster into
  `crates/effigy-release`
- move reusable version-file read/write and preview helpers that still belong
  to the release domain
- reconnect runner code as a thinner adapter over the extracted API
- update lane state and currentness surfaces honestly

## Out Of Scope

- release execution
- interactive release review shell
- unrelated cleanup outside the release seam

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

Execute `180-decide-post-release-version-and-preview-follow-up-boundary.md`.
