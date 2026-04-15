# Effigy Doctor Foundation Extraction

Date: 2026-04-16
Owner: Platform

## Summary

`138` is complete.

Effigy now has the first reusable doctor-domain slice under
[`effigy-doctor`](../../../crates/effigy-doctor/Cargo.toml). The remaining
doctor-domain logic no longer lives only in the runner module tree.

## What Changed

- added [`crates/effigy-doctor`](../../../crates/effigy-doctor/Cargo.toml)
- moved the first doctor-domain ownership there:
  - doctor contract metadata
  - manifest schema validation
  - task-reference policy helpers
- reconnected
  [`src/runner/doctor/manifest/schema.rs`](../../../src/runner/doctor/manifest/schema.rs)
  as a thin adapter over the extracted crate
- reconnected
  [`src/runner/doctor/references.rs`](../../../src/runner/doctor/references.rs)
  so it keeps runner-owned resolution glue while using the extracted
  policy/finding helpers

## Why A Boundary Decision Is Next

The reusable doctor surface is now real, but the remaining shell is smaller
and less obvious.

What remains is more orchestration-shaped:

- doctor report/render wiring
- scan-check execution
- health flow
- fix workflow and broader run orchestration

That needs an explicit boundary decision instead of another guessed
extraction.

## Current State

- active strict lane: `g02.010`
- active ready card: `139`
- queued release card: `115`

## Validation

- `cargo test -p effigy-doctor`
- `cargo test doctor --lib`
- `cargo test --test cli_output_tests doctor -- --nocapture`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `runner-owned doctor contract/schema/reference cluster`
  to `workspace-owned effigy-doctor foundation with runner adapter wiring`
- remains open:
  - post-doctor boundary classification for the remaining shell
  - release closure and `v0.3` readiness through `g02.007` once the modularization bar is met

## Next Task

Execute
[`139-decide-post-doctor-foundation-extraction-boundary.md`](../../specs/batch-cards/139-decide-post-doctor-foundation-extraction-boundary.md)
to classify the remaining doctor shell before modularization jumps again.
