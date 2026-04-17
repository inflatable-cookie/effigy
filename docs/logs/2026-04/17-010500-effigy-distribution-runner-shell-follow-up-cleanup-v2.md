# 2026-04-17 01:05:00 BST — Effigy Distribution Runner Shell Follow Up Cleanup V2

## Summary

Widened `effigy-distribution` again so the remaining distribution runner shell
is materially smaller and easier to judge honestly.

The crate now owns the reusable publish-cycle lifecycle layer:
- first-publish execution
- artifact summary writing
- artifact validation
- closeout generation
- temp-dir allocation and log discovery

`src/runner/distribution_command.rs` now adapts that layer instead of carrying
the full lifecycle block inline.

## Why This Batch

After contracts paused cleanly, distribution was still the largest remaining
disjoint runner shell not under active parallel-thread churn. The remaining
lifecycle block was still coherent domain logic, not just final CLI glue.

## What Changed

- widened `crates/effigy-distribution/src/lib.rs`
- moved the remaining publish-cycle lifecycle helpers into the crate
- deleted the displaced runner-owned lifecycle block from
  `src/runner/distribution_command.rs`
- added runner-side `DistributionExecutionError` mapping for the crate-owned
  lifecycle APIs
- repaired unrelated bootstrap workspace drift that had been breaking the full
  docs gate after a parallel extraction

## Churn Check

This was still a real extraction, not tidy-up churn. `distribution_command.rs`
dropped from `1146` lines to `860`, and the remaining file is now much closer
to preflight/GLIBC checks plus final CLI dispatch and error adaptation.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: distribution lifecycle ownership shifted from runner-local execution
  helpers to `effigy-distribution`, and the batch restored a trustworthy full
  validation boundary after unrelated bootstrap drift
- remaining open: decide whether the smaller distribution shell now pauses
  cleanly or still holds one more meaningful domain slice

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`223-decide-post-distribution-runner-shell-follow-up-cleanup-v2-boundary.md`](../../specs/batch-cards/223-decide-post-distribution-runner-shell-follow-up-cleanup-v2-boundary.md)
to decide whether the remaining distribution shell now pauses cleanly.
