# Domain Crate Boundaries And Rhai API Contract Decision

Date: 2026-04-15
Owner: Platform

## Summary

The first modularization decision batch is complete.

Effigy now has an explicit domain inventory, dependency direction, extraction
order, and Rhai adapter rule. The next batch is not direct domain splitting.
It is workspace plus `effigy-core` foundation.

## What Changed

- classified the first crate-boundary inventory in
  [`g02.010`](../../../roadmaps/g02/010-effigy-modularization-and-crate-boundaries.md)
- promoted the dependency rules and Rhai adapter posture into the active lane
- marked
  [`116-decide-domain-crate-boundaries-and-rhai-api-contract.md`](../../../specs/batch-cards/116-decide-domain-crate-boundaries-and-rhai-api-contract.md)
  complete
- opened
  [`117-implement-workspace-and-effigy-core-foundation.md`](../../../specs/batch-cards/117-implement-workspace-and-effigy-core-foundation.md)
  as the next ready card

## Decision

Use this first crate map:

- thin shell
- `effigy-core`
- `effigy-tasks`
- `effigy-distribution`
- `effigy-release`
- `effigy-containers`
- `effigy-demo`
- `effigy-env`
- `effigy-docs-policy`
- `effigy-rhai`

Supporting standalone or later workspace candidates:

- `changelog`
- `process_manager`
- `ui` / `tui`

Use this dependency direction:

- shell -> core
- shell -> domain crates
- domain crates -> core
- `effigy-rhai` -> core plus domain crates
- no sideways domain coupling except through explicit public APIs

Use this Rhai rule:

- keep `run_effigy(...)` and `run_effigy_json(...)` as the generic bridge
- typed helpers live in `effigy-rhai`
- typed helpers must call domain APIs, not private CLI wiring

## Why The First Extraction Is Core

The current codebase still centers too much in `src/lib.rs` and `src/runner/`.
Large release, demo, distribution, and container modules already justify
domain extraction, but extracting them first without a shared backbone would
just recreate the same coupling in more directories.

So the first implementation slice is:

- real Cargo workspace
- real `effigy-core` crate
- first moved shared contracts

## Current State

- active strict lane: `g02.010`
- active ready card: `117`
- queued release card: `115`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `modularization lane defined but still generic` to
  `explicit crate inventory, dependency rules, Rhai adapter posture, and first extraction order`
- remains open:
  - workspace and `effigy-core` implementation
  - later domain extraction batches
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`117-implement-workspace-and-effigy-core-foundation.md`](../../../specs/batch-cards/117-implement-workspace-and-effigy-core-foundation.md)
to establish the workspace and first shared backbone crate.
