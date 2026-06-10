# 2026-04-17 01:15:00 BST — Post Distribution Runner Shell Follow Up Cleanup V2 Boundary Decision

## Summary

The distribution seam now pauses cleanly.

After `222`, `src/runner/distribution_command.rs` is down to a smaller shell
that mostly owns:
- metadata validation
- preflight orchestration
- GLIBC floor inspection
- path resolution and final CLI error adaptation

The publish-cycle lifecycle layer is now crate-owned, so one more distribution
extraction would be fake completeness work.

## Why This Decision

The user bar is still `/src` cleanliness, but it is not “keep extracting from a
paused seam forever.” Distribution is now at the point where the remaining
code is shell-shaped enough to stop.

The next local move should shift to another disjoint seam rather than inventing
one more tiny distribution helper batch.

## Decision

- pause distribution on the current boundary
- keep `effigy-distribution` as the owner of publish-cycle lifecycle logic
- move the active lane to bootstrap next

Bootstrap is the best next local seam because:
- demo and docs are already under parallel-thread churn
- release and distribution are both paused on cleaner boundaries
- `src/runner/bootstrap_command.rs` still carries crate-adoption residue and a
  meaningful remaining runner shell

## Churn Check

This is the right place to refocus. The lane has reached the point where
distribution no longer needs another extraction card, and continuing there
would be more churn than progress.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`, `ROUTE`
- moved: distribution is now paused on an honest runner-shell boundary and the
  active cleanup focus shifts to bootstrap
- remaining open: reduce the remaining bootstrap runner shell, then re-evaluate
  the next `/src` seam

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`224-implement-effigy-bootstrap-runner-shell-follow-up-cleanup-v2.md`](../../../specs/batch-cards/224-implement-effigy-bootstrap-runner-shell-follow-up-cleanup-v2.md)
to reduce the next meaningful bootstrap runner shell slice.
