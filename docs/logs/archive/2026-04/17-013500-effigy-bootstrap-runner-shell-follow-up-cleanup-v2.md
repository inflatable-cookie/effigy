# 2026-04-17 01:35:00 BST — Effigy Bootstrap Runner Shell Follow Up Cleanup V2

## Summary

Moved the remaining bootstrap plan/result projection layer into
`crates/effigy-bootstrap` and trimmed the bootstrap adoption residue that was
still leaking warnings through the runner.

`src/runner/bootstrap_command.rs` is now a much thinner shell over
`effigy-bootstrap`, with the crate owning bootstrap plan/result rendering while
the runner keeps only command entry, callback wiring, and final error mapping.

## Why This Batch

Bootstrap was still carrying one meaningful runner-owned shell slice even after
the first crate extraction. That kept `bootstrap_command.rs` heavier than it
needed to be and left bootstrap-specific render ownership split between the
crate and the root runner.

## What Changed

- added bootstrap plan/result rendering APIs to `crates/effigy-bootstrap`
- rewired `src/runner/bootstrap_command.rs` to use crate-owned rendering
- removed duplicate local runner render helpers
- removed dead bootstrap import residue from `src/runner/manifest.rs`
- cleaned the now-unused bootstrap test helper import

## Churn Check

This was still a real shell cleanup batch, not polish churn. The runner file
dropped from `842` lines to `628`, and the remaining bootstrap shell is now
closer to honest adapter work than domain ownership.

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`
- what moved in this report: bootstrap plan/result projection moved from the
  root runner into `effigy-bootstrap`; bootstrap warning residue in runner
  imports was removed
- what remains open: post-batch boundary decision for the remaining
  `bootstrap_command.rs` shell

## Validation

- `cargo test`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`225-decide-post-bootstrap-runner-shell-follow-up-cleanup-v2-boundary.md`](../../../specs/batch-cards/225-decide-post-bootstrap-runner-shell-follow-up-cleanup-v2-boundary.md)
to decide whether bootstrap can now pause on an honest shell boundary.
