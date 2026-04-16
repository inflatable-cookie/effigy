# 181 Effigy Changelog Workspace Extraction And Release Adoption

Created: 2026-04-16
Roadmap: g02.010
Batch: effigy-changelog-workspace-extraction-and-release-adoption

## Summary
- Closed `181`.
- Moved the changelog parsing, formatting, validation, and extraction surface
  into a real `effigy-changelog` workspace crate.
- Reconnected the release and changelog paths through that crate while leaving
  the lane on a post-extraction boundary decision.

## Changes
- added `crates/effigy-changelog` and moved the changelog module tree there
- rewired the root crate to re-export `effigy-changelog` as `crate::changelog`
  so existing adopters keep one stable API surface
- removed the old root-owned `src/changelog.rs` module tree
- updated the extracted changelog tests and doctests for the new crate-relative
  module and fixture paths
- stabilized the plan-recovery fixture in
  `planning_and_selection_tests/plan_output_tests/recovery_tests.rs` so the
  available-suite assertion no longer depends on host `PATH`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `changelog parsing/formatting is still a root-crate
  library surface coupled directly to release prep` -> current
  `effigy-changelog` is a real workspace crate and release/changelog adoption
  now crosses a promoted workspace boundary
- Remaining gap: `src/runner/release_command.rs` still carries the remaining
  interactive review, render/progress, and final release-shell adapter flow

## Validation Performed
- command: `cargo fmt --all`
  - result: passed
- command: `cargo test -p effigy-changelog`
  - result: passed
- command: `cargo test`
  - result: passed after fixing the host-dependent plan-recovery suite
    assertion and the extracted changelog test fixture paths
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- `src/runner/release_command.rs` remains large enough that the next decision
  still needs to be strict about whether one more release-shell extraction is
  justified
- the root crate still re-exports `effigy-changelog`, so the boundary is real
  but the final public-surface cleanup question remains open

## Next Task
- Execute `182-decide-post-changelog-workspace-extraction-boundary.md`.
