# 2026-04-16 21:40:00 BST — Next Src Shell Cleanup Priority After Demo Boundary Decision

## Summary

The next `/src` priority is release.

## Why This Decision

After the demo runner pause, the remaining large `/src` shells are:
- `src/runner/release_command.rs`
- `src/runner/container_command.rs`
- `src/runner/docs_command.rs`
- `src/runner/contracts_command.rs`

Release is the next honest priority because:
- it is still the largest remaining runner shell
- it remains directly tied to the blocked `v0.3` release path
- it still mixes interactive review, prompt flow, text rendering, and runner
  coordination in one file
- container work is intentionally out of scope in this thread because the
  parallel container-design thread is active nearby
- docs and contracts are real seams, but they are smaller and less tied to the
  release-blocking path

## Decision

Shift the lane to one bounded release runner shell cleanup batch.

## Churn Check

This is still a meaningful batch choice, not line-count chasing. The next move
stays on a seam with both real cleanup value and direct release relevance,
without colliding with the parallel container thread.

## Vision Target Delta

- primary vision tags: `MAINT`, `RELEASE`
- moved: modularization focus from demo runner boundary closure to the next
  release runner shell reduction
- remaining open: reduce `src/runner/release_command.rs` again, then decide
  whether its remaining shell is honest enough to pause

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`199-implement-effigy-release-runner-shell-follow-up-cleanup.md`](../../specs/batch-cards/199-implement-effigy-release-runner-shell-follow-up-cleanup.md)
to reduce the next bounded release runner shell slice.
