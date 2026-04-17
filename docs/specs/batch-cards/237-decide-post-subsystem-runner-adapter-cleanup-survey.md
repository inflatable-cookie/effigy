# 237 Decide Post Subsystem Runner Adapter Cleanup Survey

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`, `g02.017` (queue job #8)
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Rerun the `/src` churn check now that the process and UI subsystems have
moved into dedicated crates. Decide whether any runner file still carries
obvious adapter residue worth one more bounded cleanup, or whether the
strict lane has reached an honest full pause.

This is `g02.017` queue job #8 — explicitly designed to run after the
cross-cutting subsystem extractions settle, because subsystem moves can
make previously-opaque runner ownership newly obvious.

## In Scope

- survey `src/runner/*.rs` shells grouped by:
  - under parallel-thread churn (demo, docs, container) — off-limits
  - paused on honest boundaries (release, distribution, bootstrap, contracts)
  - small adapter shells (changelog, script, contracts_command, bootstrap
    after tests moved)
- check whether the `effigy-process` and `effigy-ui` moves exposed any new
  adapter residue in any of the paused files
- decide between:
  - one more bounded cleanup card on a paused file
  - pause the strict lane entirely on an honest boundary

## Out Of Scope

- release execution
- reopening parallel-thread-owned seams
- speculative crate work outside the g02.017 queue

## Acceptance Criteria

- the post-subsystem runner-cleanup decision is recorded clearly
- the next move is explicit:
  - either one more bounded cleanup card is opened
  - or the strict lane pauses cleanly with a defensible boundary note

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

The `g02.010` strict lane is now paused. Resume the queued release card
[`115-implement-effigy-distribution-release-closure.md`](./115-implement-effigy-distribution-release-closure.md)
when `v0.3` closure is intended.
