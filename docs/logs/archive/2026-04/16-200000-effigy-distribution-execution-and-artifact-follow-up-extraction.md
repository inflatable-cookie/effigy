# 2026-04-16 20:00:00 BST — Effigy Distribution Execution And Artifact Follow Up Extraction

## Summary

Widened `effigy-distribution` beyond policy-only ownership.

The crate now owns the reusable artifact/log execution layer:
- artifact validation contracts
- logged step execution and log persistence
- GLIBC symbol inspection helpers
- temp artifact directory allocation
- log pattern discovery

`src/runner/distribution_command.rs` now adapts those helpers instead of owning
them inline.

## Why This Batch

Bootstrap is now paused on an honest adapter boundary. Distribution was the
next bounded root-crate product surface still carrying coherent execution and
artifact logic inside `runner`.

## What Changed

- widened `crates/effigy-distribution/src/lib.rs`
- moved artifact validation into the crate
- moved logged-step execution and log tail failure shaping into the crate
- moved GLIBC symbol inspection into the crate
- moved temp-dir and log-discovery helpers into the crate
- rewired `src/runner/distribution_command.rs` to consume the crate-owned layer

## Churn Check

This was still a meaningful extraction, not tidy-up churn. The runner file
dropped from `1352` lines to `1197`, and the extracted code now sits behind the
same crate boundary that already owned distribution policy.

## Vision Target Delta

Effigy is closer to the intended thin-shell shape. Distribution no longer keeps
its artifact/log execution cluster stranded in `runner`, which makes the
remaining shell easier to judge honestly.

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`190-decide-post-distribution-execution-and-artifact-follow-up-boundary.md`](../../../specs/batch-cards/190-decide-post-distribution-execution-and-artifact-follow-up-boundary.md)
to decide whether the remaining distribution shell can now pause cleanly.
