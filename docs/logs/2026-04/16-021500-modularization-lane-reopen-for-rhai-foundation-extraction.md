# Modularization Lane Reopen For Rhai Foundation Extraction

Date: 2026-04-16
Owner: Platform

## Summary

The previous `g02.010` pause boundary is reversed.

The modularization work is meaningful, but not yet architecturally complete
enough for `v0.3` release readiness at the bar the user wants. `g02.010` is
active again and Rhai extraction is the next batch.

## What Changed

- reopened [`g02.010`](../../roadmaps/g02/010-effigy-modularization-and-crate-boundaries.md)
- re-queued [`115`](../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)
- set [`127`](../../specs/batch-cards/127-implement-effigy-rhai-foundation-extraction.md) as the new ready card
- recorded Rhai as the next honest seam because `src/runner/script_command.rs`
  still holds a large runner-owned scripting host boundary

## Why Rhai Is Next

Rhai is not a side feature in this architecture. It is supposed to expose
domain APIs cleanly.

As long as the scripting host remains a large runner-owned integration block,
the repo is still carrying one of the important unfinished modularization
seams that would weaken the architectural completeness claim for `v0.3`.

## Current State

- active strict lane: `g02.010`
- active ready card: `127`
- queued release card: `115`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `premature modularization pause and release-lane resumption`
  to `modularization lane reopened with Rhai foundation extraction as the next batch`
- remains open:
  - Rhai foundation extraction
  - further modularization beyond the already-shipped crate slices
  - release closure and `v0.3` readiness through `g02.007` once the higher architectural bar is met

## Next Task

Execute
[`127-implement-effigy-rhai-foundation-extraction.md`](../../specs/batch-cards/127-implement-effigy-rhai-foundation-extraction.md)
to continue modularization before release closure resumes.
