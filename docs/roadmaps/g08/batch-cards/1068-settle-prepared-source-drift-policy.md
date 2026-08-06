# 1068 - Settle Prepared Source Drift Policy

Roadmap: [`../026-patch-release-lane-hardening.md`](../026-patch-release-lane-hardening.md)
Contract: [`../../../guides/051-release-orchestration.md`](../../../guides/051-release-orchestration.md)

Status: Complete
Owner: Platform
Created: 2026-08-06
Ready after: card 1067

## Purpose

Decide whether prepared-source drift is always re-prepared or can be explicitly
overridden, then make CLI, JSON, and operator guidance agree.

## Owner And Seam

`effigy-release` owns execute blockers and prepared-state validation. The CLI
runner owns flag/help rendering. This card does not authorize execution.

## Work

- inspect the current stale-state and source-drift checks independently
- choose the narrowest safe policy based on prepared-state integrity
- write a failing behavior test before implementation or documentation repair
- align human status, JSON status, help, and release guidance
- preserve irreversible execute confirmation and no-retag rules

## Acceptance

- [x] HEAD movement after prepare has one documented recovery path
- [x] `--allow-stale` scope is unambiguous in text and JSON
- [x] automated callers can determine the required recovery action
- [x] focused release unit and CLI tests pass

## Validation

- focused `effigy-release` tests
- focused release CLI corpus
- formatting and focused Clippy
- `git diff --check`

## Stop Conditions

Stop if an override would permit executing files or a commit that no longer
match the approved prepared release without an explicit integrity contract.

## Next Task

Execute card 1069 and prove the `0.9.1` candidate gates.
