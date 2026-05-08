# 185 Decide Next Src Shell Cleanup Priority After Release Boundary

Status: archived
Updated: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Choose the next honest `/src` cleanup seam now that the release-domain boundary
is good enough to stop blocking on its own, but the root crate still is not
clean enough to pause `g02.010`.

## In Scope

- assess the largest remaining shell-heavy files under `src/`
- decide which seam is the next meaningful modularization target
- leave one explicit next move:
  - either one bounded implementation card
  - or one bounded decision card if the next seam is still materially ambiguous
- update lane state and currentness surfaces honestly

## Out Of Scope

- release execution
- unrelated cross-repo rollout work
- reopening already-paused seams without a stronger reason than file size alone

## Acceptance Criteria

- the remaining `/src` pressure points are described concretely
- the chosen next seam is justified against churn and value, not just line count
- `continue` resolves through one clear next card instead of drifting

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`186-implement-effigy-bootstrap-foundation-extraction.md`](./186-implement-effigy-bootstrap-foundation-extraction.md)
to extract the next still-bounded root-crate product seam without reopening
already-paused demo or release shells.
