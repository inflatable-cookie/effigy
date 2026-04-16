# 2026-04-16 19:29:55 BST — Effigy Distribution Metadata And Closeout Follow Up Extraction

## Summary

Widened `effigy-distribution` beyond policy and artifact/log execution.

The crate now owns the reusable distribution layer around:
- metadata validation
- summary file writing
- closeout evidence gathering
- closeout report rendering

`src/runner/distribution_command.rs` now adapts those helpers instead of
carrying that cluster inline.

## Why This Batch

The previous distribution batch only removed the execution and artifact layer.
The remaining metadata, summary, and closeout path was still one coherent
distribution-domain seam stranded in `runner`.

## What Changed

- widened `crates/effigy-distribution/src/lib.rs`
- added crate-owned metadata validation contracts and checks
- added crate-owned summary writing
- added crate-owned closeout evidence loading and report rendering
- rewired `src/runner/distribution_command.rs` onto that extracted surface

## Churn Check

This was still a meaningful extraction, not tidy-up churn. The runner file
dropped from `1197` lines to `1021`, and the remaining distribution shell is
now much closer to command/task orchestration than domain ownership.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: distribution metadata/summary/closeout ownership from `runner` into
  `effigy-distribution`
- remaining open: decide whether first-publish/preflight orchestration still
  justifies one more bounded distribution extraction

## Validation

- `cargo test -p effigy-distribution`
- `cargo test distribution_command --lib`
- `cargo test --test cli_output_tests distribution`

## Next Task

Execute
[`192-decide-post-distribution-metadata-and-closeout-follow-up-boundary.md`](../../specs/batch-cards/192-decide-post-distribution-metadata-and-closeout-follow-up-boundary.md)
to decide whether the remaining distribution shell can now pause cleanly.
