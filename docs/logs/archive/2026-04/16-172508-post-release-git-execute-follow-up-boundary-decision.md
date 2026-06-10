# 178 Post Release Git Execute Follow-up Boundary Decision

Created: 2026-04-16
Roadmap: g02.010
Batch: post-release-git-execute-follow-up-boundary-decision

## Summary
- Closed `178`.
- Kept the release seam open.
- Opened `179` for one more bounded `effigy-release` extraction batch.

## Changes
- recorded that `src/runner/release_command.rs` is still not honest shell work
- classified the remaining reusable layer as the release version and mutation
  preview cluster
- updated currentness surfaces so the lane now points at `179`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `git-facing execute still runner-owned` -> current `git-facing execute is crate-owned, but version authoring and mutation preview helpers remain the next reusable release seam`
- Remaining gap: `src/runner/release_command.rs` still owns version-file read/update helpers, changelog mutation shaping, diff preview helpers, and the interactive release review shell

## Validation Performed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- `release_command.rs` is still the largest unresolved runner seam
- `179` needs to stay tightly scoped so the lane does not sprawl into a fake
  full-file rewrite

## Next Task
- Execute `179-implement-effigy-release-version-and-preview-follow-up-extraction.md`.
