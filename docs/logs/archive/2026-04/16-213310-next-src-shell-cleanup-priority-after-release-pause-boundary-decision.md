# 2026-04-16 21:33:10 BST — Next Src Shell Cleanup Priority After Release Pause Boundary Decision

## Summary

The next `/src` priority is the container runner shell.

## Why This Decision

After the release pause, the remaining larger `/src` shells are:
- `src/runner/demo_command.rs`
- `src/runner/release_command.rs`
- `src/runner/docs_command.rs`
- `src/runner/container_command.rs`
- `src/tui/demo_browser.rs`

The next honest cleanup target is container because:
- release is now paused on an honest runner-shell boundary
- demo and browser were already paused on shell/process boundaries
- docs is smaller and is already mostly adapting crate-owned docs-policy checks
- `src/runner/container_command.rs` still owns one coherent container-domain
  cluster:
  - attached session orchestration
  - stream/TUI mode shaping
  - Colima/compose execution helpers
  - shutdown/readiness flow
- the parallel container-design thread is no longer the reason to avoid the
  container seam

## Decision

Shift the lane to one bounded container runner shell cleanup batch.

## Churn Check

This is still a meaningful batch choice, not line-count chasing. The next move
stays on a real domain seam with live crate-worthy ownership still left in the
runner, rather than reopening a seam that is already paused honestly.

## Vision Target Delta

- primary vision tags: `MAINT`, `OPERATE`
- moved: modularization focus from release pause closure to the next container
  runner shell reduction
- remaining open: reduce `src/runner/container_command.rs`, then decide
  whether its remaining shell is honest enough to pause

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`207-implement-effigy-container-runner-shell-follow-up-cleanup.md`](../../../specs/batch-cards/207-implement-effigy-container-runner-shell-follow-up-cleanup.md)
to reduce the next bounded container runner shell slice.
