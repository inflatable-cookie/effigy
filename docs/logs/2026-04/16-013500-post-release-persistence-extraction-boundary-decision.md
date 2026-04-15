# Post Release Persistence Extraction Boundary Decision

Date: 2026-04-16
Owner: Platform

## Summary

`125` is complete.

The remaining release-heavy code is now judged adapter-side enough to stop
opening more `effigy-release` extraction cards by default.

## What Changed

- assessed the remaining release shell in
  [`src/runner/release_command.rs`](../../../src/runner/release_command.rs)
- recorded that the main remaining ownership is:
  - git-facing execute steps
  - verify-install temp-fixture orchestration
  - interactive text review and shell-facing progress/render flow
- treated that remainder as honest shell/runtime adapter work rather than a
  still-obvious `effigy-release` extraction seam
- opened [`126`](../../specs/batch-cards/126-decide-modularization-boundary-before-v0-3-release-resumption.md)
  as the next ready decision card

## Why The Next Batch Is The Lane Boundary Decision

The previous release extraction batches removed the clear crate-boundary debt:

- release config and gates
- release-facing models and JSON projections
- prepared-state persistence
- source fingerprint drift handling
- mutation execution helpers

What remains may still be improved later, but it does not currently justify
another release-specific extraction batch ahead of the modularization boundary
decision.

## Current State

- active strict lane: `g02.010`
- active ready card: `126`
- queued release card: `115`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `uncertain whether one more effigy-release extraction was still required`
  to `remaining release shell treated as adapter/runtime work, with the modularization boundary decision next`
- remains open:
  - modularization lane boundary decision
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`126-decide-modularization-boundary-before-v0-3-release-resumption.md`](../../specs/batch-cards/126-decide-modularization-boundary-before-v0-3-release-resumption.md)
to decide whether `g02.010` can now pause honestly and clear `g02.007` to
resume.
