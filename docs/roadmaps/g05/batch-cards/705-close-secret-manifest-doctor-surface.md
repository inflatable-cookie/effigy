# 705 - Close Secret Manifest Doctor Surface

Roadmap: [`../002-secret-manifest-and-doctor-surface.md`](../002-secret-manifest-and-doctor-surface.md)
Strict lane: [`../../../specs/077-secret-manifest-and-doctor-surface-strict-lane.md`](../../../specs/077-secret-manifest-and-doctor-surface-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Close the parser and read-only diagnostics slice before vault work starts.

## Scope

- update command reference
- update JSON payload examples
- document `.env.schema` compatibility
- document that `[secrets]` has declarations only until `g05.003`
- record validation evidence
- close `g05.002` and strict lane `077`

## Non-Goals

- no vault implementation
- no unlock implementation
- no injection implementation

## Acceptance

- [x] `g05.002` is complete
- [x] strict lane `077` is complete
- [x] front doors point to `g05.003`
- [x] next ready work is vault storage model planning/implementation

## Outcome

Closed the declaration-only secret manifest and diagnostics lane. Public docs
now include command reference coverage and an `effigy.secrets.v1` payload
example. `.env.schema` remains compatibility-only and unchanged.

## Validation

- focused parser and command tests
- docs checks for changed docs
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Open the first `g05.003` vault storage card.
