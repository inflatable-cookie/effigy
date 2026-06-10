# 2026-04-16 19:49:57 BST — Effigy Distribution First Publish And Preflight Follow Up Extraction

## Summary

Widened `effigy-distribution` again so it now owns the remaining first-publish
and preflight orchestration layer.

The crate now owns:
- preflight task execution and summary shaping
- first-publish orchestration
- publish-cycle result shaping

`src/runner/distribution_command.rs` now adapts that extracted surface instead
of carrying the publish-cycle orchestration inline.

## Why This Batch

After `191`, distribution still had one honest domain seam left in `runner`:
preflight plus first-publish orchestration. That was still product logic, not
just shell glue.

## What Changed

- widened `crates/effigy-distribution/src/lib.rs`
- added crate-owned preflight execution and result shaping
- added crate-owned first-publish execution and result shaping
- moved the generic Effigy task subprocess bridge into the crate for this path
- rewired `src/runner/distribution_command.rs` onto those extracted helpers

## Churn Check

This was still a meaningful extraction, not tidy-up churn. The runner file
dropped from `1021` lines to `745`, and the remaining distribution shell is now
much closer to adapter/orchestration code than domain ownership.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: distribution first-publish and preflight ownership from `runner` into
  `effigy-distribution`
- remaining open: decide whether the remaining distribution shell is now clean
  enough to pause

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`194-decide-post-distribution-first-publish-and-preflight-follow-up-boundary.md`](../../../specs/batch-cards/194-decide-post-distribution-first-publish-and-preflight-follow-up-boundary.md)
to decide whether the distribution seam can now pause cleanly.
