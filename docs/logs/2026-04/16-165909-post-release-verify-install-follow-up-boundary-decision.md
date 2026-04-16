# 176 Post Release Verify Install Follow-up Boundary Decision

Created: 2026-04-16
Roadmap: g02.010
Batch: post-release-verify-install-follow-up-boundary-decision

## Summary
- Closed `176`.
- Kept the release seam open.
- Opened `177` for one more bounded `effigy-release` extraction batch.

## Changes
- recorded that `src/runner/release_command.rs` is still not honest shell work
- classified the remaining reusable layer as the git-facing execute cluster
- updated currentness surfaces so the lane now points at `177`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release verify-install still runner-owned and release shell still too broad` -> current `verify-install is crate-owned, but the git-facing execute cluster remains the next release-domain seam`
- Remaining gap: `src/runner/release_command.rs` still owns branch/head/remote checks plus add/commit/tag/push orchestration

## Validation Performed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- the release seam is still large enough that `177` should stay tightly scoped
- if `177` does not materially narrow the file, the lane needs another churn
  check before opening more release-specific cards

## Next Task
- Execute `177-implement-effigy-release-git-execute-follow-up-extraction.md`.
