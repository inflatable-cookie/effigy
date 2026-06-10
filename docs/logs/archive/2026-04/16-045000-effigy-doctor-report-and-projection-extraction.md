# Effigy Doctor Report And Projection Extraction

Date: 2026-04-16
Owner: Platform

## Summary

`140` is complete.

Effigy now has the next reusable doctor-domain slice under
[`effigy-doctor`](../../../../crates/effigy-doctor/Cargo.toml). The doctor
report/result model and projection-prep contracts no longer live only in
`runner`.

## What Changed

- widened [`effigy-doctor`](../../../../crates/effigy-doctor/Cargo.toml) around:
  - doctor report/result types
  - doctor state and summary logic
  - doctor projection-prep section contracts
- removed the duplicated runner-owned report type/summary modules
- reconnected [`src/runner/doctor/render/*`](../../../../src/runner/doctor/render.rs)
  so the runner render layer now consumes extracted doctor-domain contracts
  instead of owning them inline
- kept UI notice mapping and final text/json rendering in `runner`, where that
  shell concern still belongs

## Why A Boundary Decision Is Next

The reusable doctor surface is wider now, but the remaining shell is smaller
and less obvious.

What remains is more orchestration-shaped:

- doctor command entrypoints
- doctor run workflow
- scan-check execution
- final render wiring and UI mapping

That needs an explicit boundary decision instead of another guessed
extraction.

## Current State

- active strict lane: `g02.010`
- active ready card: `141`
- queued release card: `115`

## Validation

- `cargo test -p effigy-doctor`
- `cargo test doctor --lib`
- `cargo test --test cli_output_tests doctor -- --nocapture`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `runner-owned doctor report/result/projection cluster`
  to `workspace-owned effigy-doctor report/result boundary with runner render adapters`
- remains open:
  - post-doctor boundary classification for the remaining shell
  - later release closure and `v0.3` readiness through `g02.007` once the modularization bar is met

## Next Task

Execute
[`141-decide-post-doctor-report-and-projection-extraction-boundary.md`](../../../specs/batch-cards/141-decide-post-doctor-report-and-projection-extraction-boundary.md)
to classify the remaining doctor shell before modularization jumps again.
