# 720 - Decide Varlock Adapter Or Deferral

Roadmap: [`../007-varlock-adapter-and-closeout.md`](../007-varlock-adapter-and-closeout.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-13

## Purpose

Decide whether Varlock should ship as an Effigy secret backend adapter in this
generation or be explicitly deferred behind the built-in vault.

## Scope

- review current Varlock references and existing integration points
- compare Varlock against the now-implemented Effigy secret contract
- decide one of:
  - implement external backend adapter support now
  - document Varlock as deferred
  - remove Varlock from active Effigy-facing guidance
- document the selected posture in contracts and guides
- keep the built-in vault as the default local path
- identify any `.env.schema` compatibility work needed before `g05` closeout

## Non-Goals

- no hosted secret sync
- no provider secret provisioning
- no Varlock-first contract
- no production migration
- no release commands

## Acceptance

- Varlock posture is explicit and documented.
- Built-in vault remains independent and usable without Varlock.
- External backend semantics are either defined or intentionally deferred.
- No docs imply that projects need both Varlock and the built-in vault for
  normal local development.
- Follow-up closeout work for `g05.007` is clear.

## Completed

- Audited current Varlock, `.env.schema`, and external backend references.
- Decided to defer Varlock as a live backend adapter for `g05`.
- Varlock is deferred.
- Kept `.env.schema` as native Effigy validation and task-env compatibility.
- Documented `[secrets]` plus the built-in vault as the supported local secret
  path.
- Marked `backend = "external"` as reserved parser shape only.
- Marked the old Varlock implementation handoff as historical.
- Updated the env crate package description to stop presenting the crate as a
  Varlock contract surface.

## Validation Notes

- Targeted grep for Varlock and `.env.schema` references.
- `git diff --check`.

## Validation

- docs path checks
- targeted grep for stale Varlock guidance
- command/help/docs consistency review if docs are changed

## Next Task

Execute `721` to close the `g05` secret-management suite.
