# 2026-04-16 19:52:54 BST — Post Distribution First Publish And Preflight Follow Up Boundary Decision

## Summary

The distribution seam can pause.

After `193`, [src/runner/distribution_command/mod.rs](/Users/tom/Dev/projects/effigy/src/runner/distribution_command/mod.rs)
is down to runner-shell behavior:
- repo/path resolution
- default output-path selection
- text/json payload rendering
- runner error mapping back over crate-owned distribution contracts

That is now honest adapter work, not another real `effigy-distribution`
extraction target.

## Why This Decision

The final distribution-domain slice is gone. The crate now owns:
- policy
- artifact/log execution
- metadata validation
- summary and closeout generation
- preflight orchestration
- first-publish orchestration

Keeping the lane on distribution after that would be fake completeness work.

## Decision

Pause distribution and move to the next remaining `/src` shell priority.

## Churn Check

This is the right point to stop. The file dropped to `745` lines, but the
remaining content is visibly shell-facing rather than one more stranded domain
cluster.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: distribution is now paused on an honest runner-shell boundary
- remaining open: choose the next `/src` seam to clean after distribution

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`195-decide-next-src-shell-cleanup-priority-after-distribution-boundary.md`](../../specs/batch-cards/195-decide-next-src-shell-cleanup-priority-after-distribution-boundary.md)
to choose the next remaining `/src` seam.
