# 179 Effigy Release Version And Preview Follow-up Extraction

Created: 2026-04-16
Roadmap: g02.010
Batch: effigy-release-version-and-preview-follow-up-extraction

## Summary
- Closed `179`.
- Moved the release version authoring and mutation preview layer into
  `effigy-release`.
- Left the lane on a release boundary decision instead of guessing the next
  release slice.

## Changes
- widened `crates/effigy-release/src/lib.rs` with:
  - current-version reading across Cargo, package.json, pyproject, and VERSION
  - version-file rewrite helpers for TOML, JSON, and plain-text sources
  - JSON path replacement helpers
  - version/changelog preview and mutation-detail helpers
  - diff preview helpers
- aligned `crates/effigy-release/Cargo.toml` to the workspace TOML dependency
  versions
- rewired `src/runner/release_command.rs` to use the crate-owned version and
  preview helpers
- removed the duplicate runner-owned version/update/preview helper block
- reduced `src/runner/release_command.rs` from `5097` lines to `4549`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release version authoring and mutation preview still runner-owned` -> current `version-file reading/updating and mutation preview helpers are crate-owned, runner now keeps changelog formatting plus interactive release shell behavior`
- Remaining gap: `src/runner/release_command.rs` still carries changelog parse/format coupling, interactive review flow, and final progress/render wiring

## Validation Performed
- command: `cargo fmt --all`
  - result: passed
- command: `cargo test -p effigy-release`
  - result: passed
- command: `cargo test release_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests release`
  - result: passed after restoring the original mutation-detail and diff-preview output contract
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- `release_command.rs` is still the largest unresolved runner seam
- the next decision needs to be strict about whether the remaining changelog
  coupling is crate-worthy or whether the shell is now honest enough

## Next Task
- Execute `180-decide-post-release-version-and-preview-follow-up-boundary.md`.
