# 236 Decide Post Effigy UI Extraction Boundary

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`, `g02.017` (queue job #6)
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the UI subsystem is now honest enough to pause after
`effigy-ui` absorbed the rendering primitives.

## In Scope

- inspect what still references UI rendering in the root crate
- decide whether the remaining shape is now honest adapter/wiring work
- record the decision honestly in the lane surfaces
- set the next ready card only if another bounded UI-subsystem slice is still
  justified

## Out Of Scope

- release execution
- demo/docs/container parallel cleanup
- speculative merge of `effigy-ui` into `effigy-core` (the split is explicit
  to keep the pure core free of presentation deps)

## Acceptance Criteria

- the post-`235` UI-subsystem boundary is recorded clearly
- the next move is explicit:
  - either UI pauses cleanly
  - or one more bounded slice is opened

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`237-decide-post-subsystem-runner-adapter-cleanup-survey.md`](./237-decide-post-subsystem-runner-adapter-cleanup-survey.md)
to rerun the `/src` churn check after the process + UI subsystem moves and
pick the final move for the strict lane.
