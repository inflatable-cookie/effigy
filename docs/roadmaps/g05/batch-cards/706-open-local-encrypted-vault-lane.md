# 706 - Open Local Encrypted Vault Lane

Roadmap: [`../003-local-encrypted-vault.md`](../003-local-encrypted-vault.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Open the `g05.003` implementation lane for Effigy's built-in local encrypted
vault.

## Scope

- create the strict lane for `g05.003`
- settle the exact MVP vault storage shape
- choose crate/dependency boundaries for encryption, KDF, and redaction types
- split implementation into small follow-up cards
- preserve the human-gated unlock boundary

## Non-Goals

- no vault implementation in this card
- no secret value injection
- no provider secret provisioning
- no `.env.schema` behavior change

## Acceptance

- [x] strict lane exists for `g05.003`
- [x] first implementation cards are sequenced
- [x] crypto/dependency decisions are explicit enough for implementation
- [x] no runtime behavior changes before the lane is open

## Outcome

Opened strict lane `078` for `g05.003`. The lane fixes the MVP vault as a
single encrypted local document, keeps unlock human-gated, rejects key-only
unlock, and sequences implementation through cards `707` through `711`.

## Validation

- docs path checks
- `git diff --check`

## Next Task

Execute `707` to add the secrets-domain crate/module and vault file model.
