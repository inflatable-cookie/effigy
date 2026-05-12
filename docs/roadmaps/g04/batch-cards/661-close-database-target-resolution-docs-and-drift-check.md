# 661 - Close Database Target Resolution Docs And Drift Check

Roadmap: [`../034-shared-database-target-resolution.md`](../034-shared-database-target-resolution.md)
Strict lane: [`../../../specs/070-shared-database-target-resolution-strict-lane.md`](../../../specs/070-shared-database-target-resolution-strict-lane.md)
Contract: [`../../../contracts/026-shared-database-target-resolution-contract.md`](../../../contracts/026-shared-database-target-resolution-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Close `g04.034` after seed and dump converge on the shared database target
resolution seam.

## Scope

- confirm duplicate scan no longer reports the original seed/dump helper block
- confirm contract `026` reflects the final owner and compatibility decision
- mark strict lane `070` complete
- mark roadmap `g04.034` complete
- update front doors to point to `g04.035`

## Non-Goals

- no new implementation work
- no state-domain extraction yet
- no media/object-store behavior
- no release execution

## Acceptance

- `g04.034` is complete
- strict lane `070` is complete
- contract `026` remains aligned with the implemented boundary
- next task points to `g04.035`
- validation status is recorded

## Outcome

- closed `g04.034`
- closed strict lane `070`
- kept contract `026` aligned with the implemented `effigy-data` resolver and
  runner manifest adapter boundary
- selected `g04.035` state-domain extraction as the next lane

## Validation

- `cargo test -p effigy-data` passed
- `cargo test db_services` passed
- `cargo check --bin effigy` passed
- `effigy scan duplicate-blocks --json` passed and no longer reports the
  seed/dump helper block
- `git diff --check` passed

## Next Task

Open `g04.035` state-domain extraction.
