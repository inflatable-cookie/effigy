# 233 Decide Post Effigy Process Extraction Boundary

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`, `g02.017` (queue job #4)
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide whether the process supervision seam is now honest enough to pause
after `effigy-process` absorbed the subsystem.

## In Scope

- inspect what still references process-supervision concerns in the root
  crate
- decide whether the remaining shape is now honest adapter work
- record the decision honestly in the lane surfaces
- set the next ready card only if another bounded process-subsystem slice is
  still justified

## Out Of Scope

- release execution
- demo/docs/container parallel cleanup
- speculative merge with `effigy-exec`

## Acceptance Criteria

- the post-`232` process-subsystem boundary is recorded clearly
- the next move is explicit:
  - either process supervision pauses cleanly
  - or one more bounded slice is opened

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`234-decide-next-src-shell-cleanup-priority-after-effigy-process-pause-boundary.md`](./234-decide-next-src-shell-cleanup-priority-after-effigy-process-pause-boundary.md)
to pick the next `g02.017` queue priority after pausing process supervision.
