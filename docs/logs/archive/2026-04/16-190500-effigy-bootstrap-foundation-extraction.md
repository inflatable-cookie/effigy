# 2026-04-16 19:05:00 BST — Effigy Bootstrap Foundation Extraction

## Summary

Implemented the first real bootstrap workspace boundary.

Added `crates/effigy-bootstrap` and moved bootstrap request resolution,
execution results, git checkout syncing, child bootstrap orchestration,
submodule policy application, and bootstrap-specific git/process helpers there.
`src/runner/bootstrap_command.rs` now acts as the runner adapter over that
crate, keeping manifest/task callbacks and plan/result rendering local.

## Why This Batch

`bootstrap_command.rs` was still a fully root-crate product surface while demo
and release had already been reduced to more honest shell boundaries. That made
bootstrap the next clear `/src` cleanup seam in `g02.010`.

## What Changed

- added `crates/effigy-bootstrap`
- moved bootstrap request and execution contracts into the new crate
- moved bootstrap git sync and child execution helpers into the new crate
- rewired `src/runner/bootstrap_command.rs` into a thinner adapter
- removed the now-unused runner-local bootstrap manifest re-export

## Churn Check

This was still a meaningful extraction, not cleanup churn. The runner file
dropped from `1136` lines to `839`, and bootstrap now matches the other
workspace-domain seams instead of remaining an outlier.

## Vision Target Delta

Effigy is closer to the intended thin-shell shape. Bootstrap now has a real
domain API boundary that can later support cleaner Rhai exposure and further
runner simplification without reintroducing root-crate policy ownership.

## Validation

- `cargo test -p effigy-bootstrap`
- `cargo test bootstrap_command --lib`

## Next Task

Execute
[`187-decide-post-bootstrap-foundation-extraction-boundary.md`](../../../specs/batch-cards/187-decide-post-bootstrap-foundation-extraction-boundary.md)
to decide whether bootstrap can now pause on an honest shell boundary.
