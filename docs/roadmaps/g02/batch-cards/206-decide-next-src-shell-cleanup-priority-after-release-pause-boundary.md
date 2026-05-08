# 206 Decide Next Src Shell Cleanup Priority After Release Pause Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Decide the next honest `/src` cleanup priority now that the release seam can
pause on a trustworthy runner-shell boundary.

## In Scope

- assess the remaining large shell-heavy files in `/src`
- compare them against the already-paused domain seams
- account for the parallel container thread so this lane does not create
  avoidable write-set conflict
- leave one explicit next move:
  - either one next implementation-ready shell cleanup card
  - or an explicit lane pause if `/src` is now honestly clean enough

## Out Of Scope

- reopening the release seam without a new concrete reason
- container-design roadmap work from the parallel thread
- speculative multi-seam batching

## Acceptance Criteria

- the next `/src` priority is named explicitly
- the reason for that priority is recorded concretely
- `continue` resolves through this decision instead of stale release pointers

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`207-implement-effigy-container-runner-shell-follow-up-cleanup.md`](./207-implement-effigy-container-runner-shell-follow-up-cleanup.md)
to reduce the next bounded container runner shell slice.
