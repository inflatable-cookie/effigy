# 202 Effigy Release Context And Execute Shell Follow Up Cleanup

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `effigy-release-context-and-execute-shell-follow-up-cleanup`

## Summary

Moved the release context loading and plan-collection shell out of
`src/runner/release_command.rs` and into `effigy-release`.

This batch made the runner keep the interactive prompt loop and irreversible
release dispatch shell, while the crate now owns:

- release context loading
- unreleased-count and bump calculation
- prepare-plan collection
- simulation collection
- status collection
- execute-plan collection
- prepared changelog rendering
- sync-mutation shaping

## Changes

- widened `crates/effigy-release/src/lib.rs` with crate-owned release context
  and plan-collection APIs
- added a workspace dependency from `effigy-release` to `effigy-changelog` so
  the extracted context layer uses the same changelog contract directly
- rewired `src/runner/release_command.rs` to use the crate-owned context and
  plan helpers
- removed the duplicate runner-owned `ReleaseContext` model and the local
  unreleased-count, bump, changelog-render, and sync-mutation helpers
- reduced `src/runner/release_command.rs` from `2749` lines to `2142`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release context loading and plan collection still partly runner-owned` -> current `release context and plan collection now live in effigy-release, leaving a much narrower interactive runner shell`
- Remaining gap: `src/runner/release_command.rs` still carries interactive
  review loops, prompt IO, release prepare/apply flow, release execute/apply
  flow, and release-specific progress/error adaptation

## Validation Performed

- command: `cargo fmt --all`
  - result: passed
- command: `cargo test release_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests release`
  - result: passed after one rerun because the first run hit the existing
    temp-dir bare-remote fixture collision
- command: `cargo test`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- `release_command.rs` is still large enough that the next boundary decision
  must stay strict about whether the remaining shell is now honest adapter work
  or still one more meaningful cleanup batch
- the existing release CLI test harness still has occasional temp-dir fixture
  collisions, so validation needs rerun discipline rather than treating the
  first collision as a product regression

## Next Task

- Execute `203-decide-post-release-context-and-execute-shell-follow-up-cleanup-boundary.md`.
