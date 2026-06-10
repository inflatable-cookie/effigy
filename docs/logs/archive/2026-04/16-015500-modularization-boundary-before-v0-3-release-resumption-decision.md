# Modularization Boundary Before V0.3 Release Resumption Decision

Date: 2026-04-16
Owner: Platform

## Summary

`126` is complete.

`g02.010` is now paused on a trustworthy pre-`v0.3` boundary, and `g02.007`
resumes as the active lane.

## What Changed

- assessed the shipped modularization slices against the original release-blocking goal
- recorded that the meaningful crate-boundary work is now real enough:
  - `effigy-core`
  - `effigy-tasks`
  - `effigy-manifest`
  - `effigy-containers`
  - `effigy-distribution`
  - `effigy-release`
- recorded that the remaining interleaving is accepted as shell/runtime adapter work or later follow-up, not known architecture churn that should keep release closure blocked
- paused [`g02.010`](../../../roadmaps/g02/010-effigy-modularization-and-crate-boundaries.md)
- reactivated [`g02.007`](../../../roadmaps/g02/007-distribution-release-and-consumer-rollout.md) and moved [`115`](../../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md) back to ready

## Why The Lane Can Pause

The original goal was not full architectural completion. It was to prevent
`v0.3` from freezing a runtime shape that still had obvious, already-known
crate-boundary debt.

That goal is now met:

- the shared backbone is real
- the first domain crates are real
- the release-blocking extraction cluster is real
- Rhai can target domain APIs more honestly than before

The remaining shell-side interleaving is explicit and acceptable for the
release lane to resume.

## Current State

- active strict lane: `g02.007`
- active ready card: `115`
- paused lane: `g02.010`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `modularization lane still blocking release closure`
  to `modularization lane paused on a trustworthy boundary with release closure active again`
- remains open:
  - release closure and `v0.3` readiness through `g02.007`
  - later post-`v0.3` modularization follow-up if a new crate-boundary seam justifies reopening `g02.010`

## Next Task

Execute
[`115-implement-effigy-distribution-release-closure.md`](../../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)
to resume release closure for the intended `v0.3` boundary.
