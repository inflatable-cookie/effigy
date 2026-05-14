# 739 - Switch Root And Bundle Manifest Parsers

Roadmap: [`../017-manifest-section-schema-owner-convergence.md`](../017-manifest-section-schema-owner-convergence.md)
Strict lane: [`../../../specs/082-manifest-section-schema-owner-convergence-strict-lane.md`](../../../specs/082-manifest-section-schema-owner-convergence-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-14

## Purpose

Adopt the canonical `[manifest]` owner in both root composition and bundle
defaults parsing, then prove the shared behavior with regressions.

## Scope

- switch root manifest composition to the canonical owner if the extraction kept
  a transition shim
- switch bundle defaults parsing to the same owner
- add regression tests for bundle defaults and included fragment minimum-version
  handling

## Acceptance

- root and bundle callers use the same canonical `[manifest]` owner
- the bundle-path version-floor bug is covered by regression tests
- no behavior drift is introduced in the current supported `[manifest]` fields

## Completed

- Switched bundle defaults parsing to the same canonical `[manifest]` owner used
  by root composition.
- Added a regression test proving bundle defaults accept
  `[manifest].minimum_effigy_version`.
- Kept the current supported `[manifest]` field behavior stable.

## Next Task

Execute `740` to close the lane once the canonical owner is fully adopted.
