# Keepsake Pilot Deferral Recovery

Date: 2026-04-14
Roadmap: `g02.004`
Spec: `docs/specs/004-rust-native-scripting-strict-lane.md`

## Summary

Deferred the planned Keepsake Rhai pilot because parallel Windows-VM work makes
that repo an unsafe immediate migration target. Rebound the active lane to a
new Effigy-only dogfooding batch instead of leaving the stale external-pilot
card active.

## Recovery Result

- superseded the active Keepsake pilot card
- opened a new ready card for an Effigy-only Rhai release-wrapper cluster
- kept Jetstream and Keepsake out of scope until their repo boundaries are safe
  again

## Why This Is The Right Recovery

The lane was still healthy. The invalid part was only the active next step.
Effigy still has meaningful shell-backed operator glue left, especially around
release validation wrappers, so the lane can keep moving without widening into
another repo prematurely.

## Vision Target Delta

- Primary tags: `OPERATE`, `ADOPT`
- Movement:
  - replaced a stale external pilot with a valid Effigy-only dogfooding batch
  - preserved the Rhai lane without forcing cross-repo churn
- Remaining open:
  - when the first external pilot boundary becomes safe again
