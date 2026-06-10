# 212 Effigy Release Text And Remediation Follow Up Extraction

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `effigy-release-text-and-remediation-follow-up-extraction`

## Summary

Moved the remaining release text/projection and blocker-remediation layer out
of `src/runner/release_command.rs` and into `effigy-release`.

This batch turned the previously dormant
`crates/effigy-release/src/text.rs` surface into a real adopted release API and
left the runner with a much narrower interactive shell.

## Changes

- widened `crates/effigy-release/src/lib.rs` to export the text/remediation
  layer:
  - `ReleaseBlockedStage`
  - `remediation_hints_for_blockers(...)`
  - `format_counts(...)`
  - release status/prepare/simulate/resume/execute text renderers
  - release verify-install and executed text renderers
  - release state-discarded text renderer
- rewired `src/runner/release_command.rs` onto those crate-owned text helpers
- removed the runner-owned duplicated text/remediation helpers
- reduced `src/runner/release_command.rs` from `3842` lines to `2776`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release text/projection and blocker-remediation still inline in runner` -> current `release text/remediation layer lives in effigy-release, leaving a much narrower release runner shell`
- Remaining gap: `src/runner/release_command.rs` still carries interactive
  prompt flow, release context loading, and final runner-side apply/dispatch
  wiring

## Validation Performed

- command: `cargo fmt --all`
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

- the release shell is materially smaller, but it still has enough interactive
  runner logic that the next boundary decision must stay strict about whether
  one more cleanup slice is justified
- the remaining warning residue in the docs pass is now outside the release
  seam, in the parallel demo shell work

## Next Task

- Execute `213-decide-post-release-text-and-remediation-follow-up-boundary.md`.
