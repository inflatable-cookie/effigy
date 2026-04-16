# 172 Post Demo Runtime Control And Process Follow-up Boundary Decision

Created: 2026-04-16
Roadmap: g02.010
Batch: post-demo-runtime-control-and-process-follow-up-boundary-decision

## Summary
- Closed `172`.
- Kept the demo runner seam open.
- Opened `173` for one more bounded `effigy-demo` extraction batch.

## Changes
- recorded that `src/runner/demo_command.rs` is not yet honest shell work
- classified the remaining reusable layer as managed runtime state, backend
  classification/projection helpers, and stop/attach capability shaping
- updated currentness surfaces so the lane now points at `173`

## Vision Target Delta
- Primary tags: `MAINT`, `CONTRACT`, `ROUTE`
- Movement: baseline `demo runner still mixed between crate-owned runtime logic and runner shell work` -> current `demo runner narrowed again with one explicit reusable managed-runtime/backend slice left`
- Remaining gap: `src/runner/demo_command.rs` still owns managed runtime state, backend classification/projection helpers, and stop/attach runtime shaping

## Validation Performed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks
- if `173` does not materially shrink the managed runtime shell, the lane will
  need a churn check before opening another demo-specific card

## Next Task
- Execute `173-implement-effigy-demo-managed-runtime-and-backend-follow-up-extraction.md`.
