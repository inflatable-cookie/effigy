# 707 - Add Secret Domain And Vault File Model

Roadmap: [`../003-local-encrypted-vault.md`](../003-local-encrypted-vault.md)
Strict lane: [`../../../specs/078-local-encrypted-vault-strict-lane.md`](../../../specs/078-local-encrypted-vault-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Add the internal domain boundary and serialized vault file model before adding
encryption or CLI mutation.

## Scope

- create the secrets domain crate/module boundary
- define redacted secret value wrapper types
- define versioned vault envelope structs
- define encrypted payload record structs
- define file permission diagnostic helpers
- add serialization/deserialization tests with placeholder ciphertext bytes

## Non-Goals

- no encryption implementation
- no passphrase prompt
- no CLI mutation commands
- no unlock cache
- no runtime injection

## Acceptance

- [x] vault file model round-trips through serde
- [x] secret values are redacted in `Debug` and display paths
- [x] malformed vault model diagnostics are clear
- [x] unsafe file permission checks are represented where platform support allows

## Outcome

Added the `effigy-secrets` crate as the domain boundary for vault storage
models. It owns the versioned vault envelope, placeholder encrypted payload
model, redacted secret value wrappers, plaintext record model for later
encryption, validation diagnostics, and vault file permission checks.

## Validation

- focused secrets-domain tests
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `708` to add vault crypto round-trip support.
