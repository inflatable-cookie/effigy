# g08 Roadmap Consolidation

Roadmap: `g08.009`
Batch cards: `1037` through `1050`
Date: 2026-06-05

## Summary

Consolidated the 2026-06-04 code-quality and dead-code scan sweep back into
one roadmap.

The drift was structural:

- `g08.010` through `g08.017` split one problem space into child roadmaps.
- batch cards `1038` through `1050` pointed at those child roadmaps instead of
  the sweep parent.
- roadmap front doors reported `g08` complete through `g08.017`, which made
  the batch-card sequence look like separate milestone planning.

## Changes

- Kept `g08.009` as the single roadmap for the sweep.
- Moved the useful child-roadmap state into `g08.009` as batch-slice history.
- Repointed batch cards `1038` through `1050` to `g08.009`.
- Removed roadmap files `g08.010` through `g08.017`.
- Updated roadmap front doors and generation index to report `g08` complete
  through `g08.009`.
- Normalized stale references in same-day sweep logs so they no longer point
  at deleted roadmap IDs.

## Current State

`g08.009` now covers the whole code-quality boundary sweep:

- command and Rhai descriptor convergence
- container `up` phase boundary cleanup
- repo-marker/root-rule convergence
- selected duplicate-block cleanup
- boundary/dead-code scan self-adoption
- dead-code Rust signal repair
- residual dead-code false-positive burn-down

The current dead-code scan evidence remains:

- findings: 0
- isolated files: 0
- unreferenced symbols: 0

No active ready card remains for the sweep.

## Vision Target Delta

- Tags touched: `MAINT`, `CONTRACT`
- Baseline: one sweep was represented as multiple child roadmap files.
- Current: one roadmap owns the sweep, with batch cards carrying execution
  detail.
- Open: none for this consolidation.

## Next Task

No active ready card remains for the sweep.
