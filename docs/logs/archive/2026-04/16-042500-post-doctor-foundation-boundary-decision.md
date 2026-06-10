# Post-Doctor Foundation Boundary Decision

Date: 2026-04-16
Owner: Platform

## Summary

`139` is complete.

The remaining doctor shell still justifies another `effigy-doctor` extraction
batch. The next reusable seam is doctor report/result ownership.

## Decision

Do not pause doctor yet.

Treat this remaining cluster as still reusable doctor-domain API rather than
mere CLI glue:

- [`src/runner/doctor/report/types.rs`](../../../../src/runner/doctor/report/types.rs)
- [`src/runner/doctor/report/state.rs`](../../../../src/runner/doctor/report/state.rs)
- [`src/runner/doctor/report/summary.rs`](../../../../src/runner/doctor/report/summary.rs)
- [`src/runner/doctor/render/contracts.rs`](../../../../src/runner/doctor/render/contracts.rs)

What remains beyond that is more orchestration-shaped:

- doctor command entrypoints
- doctor run workflow
- scan-check execution
- text/json rendering

## Why Another Doctor Slice Is Justified

The first doctor extraction moved the basic contract and policy layer:

- check metadata
- manifest schema validation
- task-reference policy

But the doctor result model still lives heavily in `runner`, and multiple
doctor paths depend on it. That makes it a stronger next seam than jumping to
release or generic shell cleanup.

## Current State

- active strict lane: `g02.010`
- active ready card: `140`
- queued release card: `115`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `doctor boundary uncertain after first effigy-doctor slice`
  to `doctor boundary classified as still needing report/result extraction`
- remains open:
  - doctor report/projection extraction
  - later release closure and `v0.3` readiness through `g02.007` once the modularization bar is met

## Next Task

Execute
[`140-implement-effigy-doctor-report-and-projection-extraction.md`](../../../specs/batch-cards/140-implement-effigy-doctor-report-and-projection-extraction.md)
to move the reusable doctor report/result cluster out of `runner`.
