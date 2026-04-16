# 214 Effigy Release Context And Plan Follow Up Cleanup V3

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `effigy-release-context-and-plan-follow-up-cleanup-v3`

## Summary

Moved the remaining release context-loading and plan-collection layer out of
`src/runner/release_command.rs` and into `effigy-release`.

This batch turned release context loading, status/prepare/simulate/execute-plan
collection, prepared changelog shaping, and sync-mutation shaping into
crate-owned APIs and left the runner much closer to an honest interactive shell.

## Changes

- widened `crates/effigy-release/src/lib.rs` so `effigy-release` now owns:
  - `ReleaseContext`
  - `load_release_context(...)`
  - `collect_release_status(...)`
  - `build_release_prepare_plan(...)`
  - `collect_release_simulation(...)`
  - `collect_release_execute_plan(...)`
  - prepared changelog rendering
  - sync mutation shaping
- rewired `src/runner/release_command.rs` onto those crate-owned APIs
- removed the runner-owned duplicated release context and plan helpers
- reduced `src/runner/release_command.rs` from `2776` lines to `2181`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release context loading and plan collection still inline in runner` -> current `release context/plan collection lives in effigy-release, leaving a much narrower release runner shell`
- Remaining gap: `src/runner/release_command.rs` still carries interactive
  prepare/execute/resume review flow, prompt and section-browser IO, and final
  runner-side apply/dispatch wiring

## Validation Performed

- command: `cargo test release_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests release`
  - result: passed
- command: `cargo fmt --all`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- the release shell is materially smaller, but it still needs a strict boundary
  decision before the lane can honestly pause
- the only remaining warning residue in validation is outside the release seam,
  in the parallel demo work

## Next Task

- Execute `215-decide-post-release-context-and-plan-follow-up-cleanup-v3-boundary.md`.
