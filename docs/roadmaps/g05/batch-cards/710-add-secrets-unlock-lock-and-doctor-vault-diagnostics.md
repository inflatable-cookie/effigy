# 710 - Add Secrets Unlock Lock And Doctor Vault Diagnostics

Roadmap: [`../003-local-encrypted-vault.md`](../003-local-encrypted-vault.md)
Strict lane: [`../../../specs/078-local-encrypted-vault-strict-lane.md`](../../../specs/078-local-encrypted-vault-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Ready
Owner: Platform
Created: 2026-05-12

## Purpose

Add explicit unlock/lock commands and extend `secrets doctor` with vault-state
diagnostics.

## Scope

- add `effigy secrets unlock`
- add `effigy secrets lock`
- keep unlock state scoped to the current invocation unless implementation
  proves a safer narrow cache is needed
- update `secrets doctor` to report:
  - no vault
  - locked vault
  - unlocked vault
  - corrupt vault
  - unsafe permissions
  - missing required values
  - undeclared stored values

## Non-Goals

- no daemon
- no cross-invocation unlock cache
- no runtime injection
- no provider secret provisioning

## Acceptance

- unlock requires operator participation
- lock clears any invocation-local unlock state
- doctor distinguishes all MVP vault states without exposing values
- corrupt and unsafe vault states fail closed
- missing required values block clearly

## Validation

- CLI parser tests
- runner command tests
- vault-state fixture tests
- redaction tests
- `cargo check --all-targets`
- `git diff --check`

## Next Task

Execute `711` to document and close `g05.003`.
