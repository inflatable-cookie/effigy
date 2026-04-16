# 183 Implement Effigy Release Review And Text Projection Follow-up Extraction

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract the remaining release-specific review/menu/text-projection layer out of
`src/runner/release_command.rs` so the release seam can be judged on a clean
final shell boundary instead of one last large mixed runner file.

## In Scope

- move release review state and menu/progress text shaping into
  `effigy-release`
- move release-specific text projection helpers for prepare/simulate/resume
  and execute into the extracted boundary where that does not force generic CLI
  shell concerns into the crate
- reduce `src/runner/release_command.rs` materially
- update lane state and currentness surfaces honestly

## Out Of Scope

- release closure
- generic CLI shell/help/render cleanup outside the release seam
- unrelated modularization outside release and its immediate adopters

## Acceptance Criteria

- `src/runner/release_command.rs` no longer owns the bulk of the release
  review/text-projection layer
- the remaining release shell is described honestly after the batch
- the next move is a strict boundary decision, not another guessed slice

## Validation

- `cargo test -p effigy-release`
- `cargo test release_command --lib`
- `cargo test --test cli_output_tests release`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`184-decide-post-release-review-and-text-projection-follow-up-boundary.md`](./184-decide-post-release-review-and-text-projection-follow-up-boundary.md)
to decide whether the remaining release shell is finally honest enough to pause
for `g02.007` release resumption.
