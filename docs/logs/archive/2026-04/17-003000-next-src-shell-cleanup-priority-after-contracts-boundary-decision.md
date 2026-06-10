# 2026-04-17 003000 - Next Src Shell Cleanup Priority After Contracts Boundary Decision

## Summary

Completed `221` by choosing the next substantial `/src` cleanup priority after
pausing the contracts seam.

## Decision

Choose distribution next.

Reason:

- `src/runner/distribution_command.rs` is the largest remaining disjoint runner
  shell after excluding demo and docs surfaces that are already under parallel
  thread churn
- release was just paused on an honest boundary and is not the next best seam
- distribution still has enough shell mass to justify one more real bounded
  cleanup batch

## Next Task

Execute
[`222-implement-effigy-distribution-runner-shell-follow-up-cleanup-v2.md`](../../../specs/batch-cards/222-implement-effigy-distribution-runner-shell-follow-up-cleanup-v2.md)
to reduce the next meaningful distribution runner shell slice.
