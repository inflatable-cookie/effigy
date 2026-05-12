# 704 - Add Secrets List And Doctor

Roadmap: [`../002-secret-manifest-and-doctor-surface.md`](../002-secret-manifest-and-doctor-surface.md)
Strict lane: [`../../../specs/077-secret-manifest-and-doctor-surface-strict-lane.md`](../../../specs/077-secret-manifest-and-doctor-surface-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Add the first read-only command surface for declared secrets.

## Scope

- add CLI parsing for:
  - `effigy secrets list [--json]`
  - `effigy secrets doctor [--json]`
- render declared secret names, targets, required flags, backend, and safe
  metadata
- report missing `[secrets]` as an empty/no-contract state
- report missing backend config as diagnostics, not value lookup failures
- keep output value-free by construction

## Non-Goals

- no vault file creation
- no secret values
- no unlock
- no runtime injection

## Acceptance

- [x] commands work against repos with no `[secrets]`
- [x] commands work against repos with declared keys
- [x] JSON output has no value fields
- [x] invalid config errors clearly
- [x] help and command reference mention the read-only surface

## Outcome

Added `effigy secrets list` and `effigy secrets doctor` as read-only
declaration commands. Both resolve the active repo manifest, report declared
secret names/targets/backend metadata, and never read, unlock, inject, or print
secret values.

## Validation

- CLI parser tests
- runner command tests
- JSON payload tests
- docs path checks
- `git diff --check`

## Next Task

Execute `705` to document and close `g05.002`.
