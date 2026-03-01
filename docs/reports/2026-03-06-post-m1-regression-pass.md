# Post-M1 Regression Pass

Date: 2026-03-06
Owner: betterthanclay
Related roadmap: post-M1 hardening

## Scope
- Run regression confidence checks for lock/watch/migrate/doctor core flows.
- Confirm no critical regressions remain before release-readiness checkpoint.

## Changes
- Pending execution.

## Validation
- command: `cargo test`
  - result: pending
- command: `effigy watch ...` (targeted smoke)
  - result: pending
- command: `effigy init` / `effigy migrate ...` (targeted smoke)
  - result: pending
- command: `effigy doctor ...` (targeted smoke)
  - result: pending

## Risks / Follow-ups
- Pending execution.

## Next
- Block or advance release-readiness checkpoint based on regression outcomes.
