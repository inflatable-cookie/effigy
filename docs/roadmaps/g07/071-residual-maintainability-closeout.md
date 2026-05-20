# g07.071 - Residual Maintainability Closeout

Status: Planned
Depends on: `g07.070`

## Goal

Close the residual-maintainability extension to `g07` with updated scan
evidence, focused validation, and an honest statement of what still is not
worth doing.

## Scope

- rerun the same god-file and duplicate-block scans used to open the reopened
  `g07` tranche
- compare deltas against the `g07` closeout baseline
- run focused tests for every touched surface
- run broad repo QA once the batch is complete
- close the generation or record the exact reason it should remain open

## Guardrails

- no new cleanup scope except tiny closeout fixes
- no unsupported claim that all duplication is gone
- no release mutations

## Acceptance Criteria

- final scan deltas are recorded
- broad QA result is recorded or any blocker is explicit
- remaining debt is sorted into follow-up, defer, or not worth doing
- roadmap front doors can close or roll forward cleanly

## Next Task

No active ready card until the closeout finishes.
