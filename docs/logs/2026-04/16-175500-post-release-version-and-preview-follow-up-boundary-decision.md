# 180 Post Release Version And Preview Follow-up Boundary Decision

Created: 2026-04-16
Roadmap: g02.010
Batch: post-release-version-and-preview-follow-up-boundary-decision

## Summary
- Closed `180`.
- Kept the release seam open.
- Opened `181` for a changelog workspace extraction rather than another tiny
  `effigy-release` helper move.

## Changes
- recorded that `src/runner/release_command.rs` is smaller but still not honest
  shell work
- classified the remaining reusable layer as changelog parsing/formatting
  coupling rather than one more isolated release helper cluster
- updated currentness surfaces so the lane now points at `181`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release version authoring and preview still runner-owned` -> current `version authoring and preview are crate-owned, but release prep still depends on a root-crate changelog surface`
- Remaining gap: `src/changelog.rs` and `src/runner/release_command.rs` still form one reusable changelog/release seam that should move behind a workspace crate boundary

## Validation Performed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- `release_command.rs` remains the largest unresolved runner seam
- if the changelog extraction does not materially reduce root-crate ownership,
  the lane needs another churn check before more release-specific cards open

## Next Task
- Execute `181-implement-effigy-changelog-workspace-extraction-and-release-adoption.md`.
