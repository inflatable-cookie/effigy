# g05.019 - Schema Shape Regression Proof And Closeout

Status: Planned
Depends on: `g05.017`, `g05.018`

## Goal

Close the schema-shape consolidation suite with proof that the duplicated owner
surfaces are reduced and the current supported TOML syntax still behaves the
same.

## Scope

- rerun focused manifest, bundle, bootstrap, and state schema tests
- document any intentionally retained duplicate schema owners and why they stay
- refresh planning/currentness surfaces if this suite becomes the active `g05`
  runway

## Non-Goals

- no new syntax work
- no adjacent runtime refactors disguised as validation

## Acceptance Criteria

- focused regression proof is recorded
- retained duplicate schema owners, if any, are explicit and justified
- the suite closes without stale planning pointers

## Next Task

Use this as the closeout lane once the owner-convergence slices land.
