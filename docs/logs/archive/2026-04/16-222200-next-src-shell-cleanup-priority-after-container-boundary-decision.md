# 2026-04-16 22:22:00 BST — Next Src Shell Cleanup Priority After Container Boundary Decision

## Summary

The next `/src` priority is release.

## Why This Decision

After the container pause, the remaining large `/src` shells are:
- `src/runner/release_command.rs`
- `src/runner/demo_command.rs`
- `src/tui/demo_browser.rs`
- `src/runner/bootstrap_command.rs`
- `src/runner/distribution_command.rs`
- `src/runner/docs_command.rs`

Release is the next honest priority because:
- it is still the largest remaining runner shell
- it remains directly tied to the blocked `v0.3` release path
- it still mixes interactive review, prompt flow, progress/error adaptation,
  and runner coordination in one file
- the parallel docs thread is active, so choosing docs here would create
  avoidable write-set conflict
- demo is still a large shell, but release has the stronger product and
  release-readiness coupling

## Decision

Shift the lane to one more bounded release runner-shell cleanup batch.

## Churn Check

This is still a meaningful batch choice, not line-count chasing. The next move
stays on a seam with both real cleanup value and direct release relevance,
without colliding with the active parallel docs slice.

## Vision Target Delta

- primary vision tags: `MAINT`, `RELEASE`
- moved: modularization focus from the paused container seam to the next
  release runner shell reduction
- remaining open: reduce `src/runner/release_command.rs` again, then decide
  whether its remaining shell is honest enough to pause

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`210-implement-effigy-release-runner-shell-follow-up-cleanup-v2.md`](../../../specs/batch-cards/210-implement-effigy-release-runner-shell-follow-up-cleanup-v2.md)
to reduce the next bounded release runner shell slice.
