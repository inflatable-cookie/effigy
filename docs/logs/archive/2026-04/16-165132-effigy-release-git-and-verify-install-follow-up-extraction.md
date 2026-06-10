# 175 Effigy Release Git And Verify Install Follow-up Extraction

Created: 2026-04-16
Roadmap: g02.010
Batch: effigy-release-git-and-verify-install-follow-up-extraction

## Summary
- Closed `175`.
- Moved the verify-install execution cluster into `effigy-release`.
- Left the lane on a release boundary decision instead of guessing the next
  release slice.

## Changes
- added crate-owned verify-install helpers in `crates/effigy-release/src/lib.rs`
- moved tag resolution, repo-url normalization, temp fixture setup, fixture
  writing, and verification-step execution into `effigy-release`
- rewired `src/runner/release_command.rs` to adapt the extracted release path
- updated the release tests to use the crate-owned normalization helper
- reduced `src/runner/release_command.rs` from `5581` lines to `5346`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release verify-install still runner-owned` -> current `verify-install execution is crate-owned, runner now keeps only the repo/remote discovery wrapper around that path`
- Remaining gap: `src/runner/release_command.rs` still carries git-facing execute orchestration, prepared-state review, and interactive release flow shell work

## Validation Performed
- command: `cargo fmt --all`
  - result: passed
- command: `cargo test -p effigy-release`
  - result: passed
- command: `cargo test release_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests release`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- `release_command.rs` is still the largest unresolved runner seam
- the next decision should check whether the remaining release mass is still
  crate-worthy or finally mostly interactive shell behavior

## Next Task
- Execute `176-decide-post-release-verify-install-follow-up-boundary.md`.
