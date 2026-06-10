# Modularization Lane Activation Before v0.3 Release

Date: 2026-04-15
Owner: Platform

## Summary

The release lane was not removed, but it is no longer the active strict lane.

The user wants modularization done before the next release so Effigy does not
ship a release-closure checkpoint and then immediately replace the runtime
shape.

This batch activated `g02.010` for crate-boundary architecture work and left
`g02.007` queued behind it.

## What Changed

- added `g02.010`:
  [`010-effigy-modularization-and-crate-boundaries.md`](../../../roadmaps/g02/010-effigy-modularization-and-crate-boundaries.md)
- added the active strict lane:
  [`010-effigy-modularization-and-crate-boundaries-strict-lane.md`](../../../specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md)
- added the first ready card:
  [`116-decide-domain-crate-boundaries-and-rhai-api-contract.md`](../../../specs/batch-cards/116-decide-domain-crate-boundaries-and-rhai-api-contract.md)
- kept
  [`115-implement-effigy-distribution-release-closure.md`](../../../specs/batch-cards/115-implement-effigy-distribution-release-closure.md)
  queued as the follow-on release card
- updated the roadmap/spec/readme currentness surfaces to advertise `116` as
  the next move and `115` as queued release work

## Why The Shift Happened

Recent batches tightened real release prep:

- local Linux release rehearsal is real
- Rhai now has in-process Effigy dispatch and first typed container helpers

That left the release lane technically ready to move on, but not strategically
ready. The user wants reusable domain boundaries landed before `v0.3`, and the
current codebase is already showing too much interleaving across tasks,
distribution, containers, scripting, and release orchestration.

## Current State

- active strict lane: `g02.010`
- active ready card: `116`
- queued release card: `115`
- `g02.007` stays alive, but release closure now resumes only after the
  modularization lane reaches a trustworthy pre-release boundary

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `active release-closure lane with 115 as next move` to
  `active modularization lane with 116 as next move and 115 explicitly queued`
- remains open:
  - domain inventory and crate-boundary decisions
  - first extraction order
  - eventual resume of release closure for `v0.3`

## Next Task

Execute
[`116-decide-domain-crate-boundaries-and-rhai-api-contract.md`](../../../specs/batch-cards/116-decide-domain-crate-boundaries-and-rhai-api-contract.md)
to classify the first modularization boundary and leave the first extraction
batch explicitly.
