# 708 - Add Vault Crypto Round Trip

Roadmap: [`../003-local-encrypted-vault.md`](../003-local-encrypted-vault.md)
Strict lane: [`../../../specs/078-local-encrypted-vault-strict-lane.md`](../../../specs/078-local-encrypted-vault-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Implement authenticated encryption and passphrase-derived unlock material for
the local vault document.

## Scope

- add memory-hard passphrase derivation
- add authenticated encryption/decryption for the vault payload
- generate salts and nonces from OS randomness
- fail closed on wrong passphrase, corrupt payload, or unsupported version
- keep test-only passphrase fixtures out of public CLI flags

## Non-Goals

- no SSH key wrapping yet unless it fits cleanly without weakening safety
- no CLI mutation commands
- no runtime injection
- no daemon or cross-invocation cache

## Acceptance

- [x] vault payload encrypts and decrypts with the correct passphrase
- [x] wrong passphrase fails without exposing plaintext
- [x] corrupt payload fails closed
- [x] unsupported algorithm/version fails clearly
- [x] redaction tests cover error/debug/report paths

## Outcome

Added Argon2id-derived vault keys and XChaCha20-Poly1305 authenticated
encryption/decryption to `effigy-secrets`. Salt and nonce material are
generated from OS randomness for normal encryption, with deterministic
test-only material helpers for fixtures.

## Validation

- crypto round-trip tests
- wrong-passphrase tests
- corrupt-file tests
- redaction tests
- `cargo check --all-targets`
- `git diff --check`

## Next Task

Execute `709` to add `secrets init`, `set`, and `unset`.
