# 2026-03-11 10:49:00 - changelog extract release-notes precutover

## Summary
- Added end-to-end CLI coverage for `effigy changelog extract` on release-note
  shaped input, including both successful extraction and missing-version
  failure.
- Updated release-note and release-protocol docs to use
  `effigy changelog extract CHANGELOG.md --version X.Y.Z` as the preferred
  baseline generator for human-reviewed release notes.
- Left `.github/workflows/` untouched; this batch prepares the workflow swap
  without taking it.

## Why
- The remaining `027` workflow task requires explicit approval before editing
  `release-binaries.yml`.
- This batch removes the design ambiguity first, so the later workflow change is
  only an approved wiring change to a documented and tested command.

## Verification
- `cargo fmt --all`
- `cargo test --test cli_output_tests cli_changelog_extract_ -- --nocapture`
- `git diff --check`
