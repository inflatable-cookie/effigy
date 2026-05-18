# g07.020 - Scan Cost Closeout Proof

Status: Complete
Depends on: `g07.017`

## Goal

Close the scan-cost lane with measured before/after proof and explicit
residual limits.

## Scope

- rerun the no-op index proof
- rerun `graph status --json` timing
- compare against the `g07.013` closeout
- record what still dominates cost after the scan reductions

## Guardrails

- no hand-wavy “feels faster” closeout
- no mixing query wins into a scan-cost claim unless they came from the same
  changes

## Acceptance

- closeout log records the scan-cost delta and retained limits explicitly

## Next Task

No active task remains in this roadmap.
