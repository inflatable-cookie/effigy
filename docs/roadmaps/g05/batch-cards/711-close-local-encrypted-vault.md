# 711 - Close Local Encrypted Vault

Roadmap: [`../003-local-encrypted-vault.md`](../003-local-encrypted-vault.md)
Strict lane: [`../../../specs/078-local-encrypted-vault-strict-lane.md`](../../../specs/078-local-encrypted-vault-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Pending
Owner: Platform
Created: 2026-05-12

## Purpose

Close `g05.003` before any runtime injection work starts.

## Scope

- update command reference
- update JSON payload examples
- update Rustdoc or module docs for the vault domain
- record validation evidence
- close strict lane `078`
- move front doors to `g05.004`

## Non-Goals

- no task injection
- no Rhai injection
- no deploy injection
- no container startup injection
- no `.env` export

## Acceptance

- `g05.003` is complete
- strict lane `078` is complete
- docs explain human-gated unlock and no key-only unlock
- next ready work is `g05.004`

## Validation

- focused vault tests
- docs checks
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Open the first `g05.004` task/Rhai/deploy injection card.
