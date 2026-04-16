# 204 Effigy Release Interactive And Apply Shell Follow Up Cleanup

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `effigy-release-interactive-and-apply-shell-follow-up-cleanup`

## Summary

Moved the release apply/orchestration shell out of
`src/runner/release_command.rs` and into `effigy-release`.

This batch made the runner keep the interactive prompt loop and final terminal
IO shell, while the crate now owns:

- release prepare apply orchestration
- release execute apply orchestration
- release gate progress orchestration
- post-release instruction shaping

## Changes

- widened `crates/effigy-release/src/lib.rs` with crate-owned apply/orchestration
  APIs:
  - `execute_release_prepare(...)`
  - `execute_release(...)`
  - `run_release_gates_with_progress(...)`
- moved the release progress duration formatter into the crate so the progress
  contract lives with the release gate runner
- rewired `src/runner/release_command.rs` onto the crate-owned apply/orchestration
  helpers
- reduced `src/runner/release_command.rs` from `2142` lines to `1782`
- fixed adjacent container compile residue from the completed parallel thread so
  the full validation round reflected the release batch instead of stale
  integration breakage

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release interactive/apply orchestration still runner-owned` -> current `release apply/orchestration lives in effigy-release, leaving a much narrower interactive runner shell`
- Remaining gap: `src/runner/release_command.rs` still carries interactive
  prepare/execute/resume review loops, prompt and section-browser IO, version
  override validation, and final runner-side release command dispatch

## Validation Performed

- command: `cargo fmt --all`
  - result: passed
- command: `cargo test release_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests release`
  - result: passed
- command: `cargo test`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- `release_command.rs` is smaller, but still large enough that the next
  boundary decision must stay strict about whether the remaining interactive
  shell is now honest adapter work
- the release CLI harness still has occasional temp-dir bare-remote collisions,
  so rerun discipline remains part of trustworthy validation

## Next Task

- Execute `205-decide-post-release-interactive-and-apply-shell-follow-up-cleanup-boundary.md`.
