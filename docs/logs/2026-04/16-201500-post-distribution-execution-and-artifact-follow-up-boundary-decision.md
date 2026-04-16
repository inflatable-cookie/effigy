# 2026-04-16 20:15:00 BST — Post Distribution Execution And Artifact Follow Up Boundary Decision

## Summary

Kept the distribution seam open.

The artifact/log execution slice is now crate-owned, but
`src/runner/distribution_command.rs` still carries another reusable
distribution-domain layer.

## Decision

Distribution does not pause yet.

The next ready batch is one more bounded distribution extraction focused on the
remaining metadata/preflight/summary/closeout/first-publish cluster.

## Why

- the runner no longer owns the artifact/log execution helpers
- but it still owns:
  - metadata validation
  - preflight orchestration
  - summary shaping
  - closeout generation
  - most of first-publish orchestration
- that is still domain logic, not just final shell dispatch and rendering

## Vision Target Delta

The distribution seam is materially smaller, but pausing now would still leave
too much reusable product logic stranded in `runner`. One more bounded
distribution slice is justified before the lane shifts again.

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`191-implement-effigy-distribution-metadata-and-closeout-follow-up-extraction.md`](../../specs/batch-cards/191-implement-effigy-distribution-metadata-and-closeout-follow-up-extraction.md).
