# 218 Decide Next Src Shell Cleanup Priority After Release Final Pause Boundary

Status: complete
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next honest `/src` shell cleanup priority now that the release seam
is paused on a trustworthy runner-shell boundary.

## In Scope

- reassess the remaining heavy `/src` seams after the release pause
- compare the next best shell-cleanup targets honestly
- leave one explicit next move:
  - either open the next bounded cleanup batch
  - or explicitly pause if no remaining seam still earns active work

## Out Of Scope

- release execution
- reopening the release seam without a new concrete blocker
- broad roadmap churn outside the active lane

## Acceptance Criteria

- the next `/src` cleanup priority is explicit
- the reason for that priority is concrete
- `continue` resolves through this decision instead of stale release pointers

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`219-implement-effigy-contracts-foundation-extraction.md`](./219-implement-effigy-contracts-foundation-extraction.md)
to extract the contracts command surface into a dedicated workspace crate.
