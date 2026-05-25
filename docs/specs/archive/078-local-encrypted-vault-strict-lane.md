# 078 - Local Encrypted Vault Strict Lane

Roadmap: [`g05.003`](../roadmaps/g05/003-local-encrypted-vault.md)
Contract: [`032-secret-and-local-config-management-contract.md`](../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Implement Effigy's built-in local encrypted vault for developer secrets.

This lane starts after the declaration-only `[secrets]` surface. It may store
and decrypt local secret values, but only behind an explicit human-gated unlock
model.

## MVP Storage Decision

The MVP uses one encrypted vault document at `[secrets.vault].path`.

Required shape:

- versioned envelope with algorithm metadata
- random salt for passphrase derivation
- random nonce for payload encryption
- authenticated encrypted payload
- encrypted payload contains key/value records keyed by declared secret name
- cleartext metadata must never include secret values or derived value hashes
- file permissions are checked before reading and after writing

## Crypto Boundary

Use conservative library primitives through a small secrets-domain module or
crate. Do not design custom cryptography.

Initial target:

- Argon2id passphrase KDF via `argon2`
- XChaCha20-Poly1305 authenticated encryption via `chacha20poly1305`
- random nonces and salts from OS randomness
- constant redaction wrapper for values in debug/text/report paths

If the exact Rust crates change during implementation, update this spec in the
same card that introduces the dependency. The behavioral contract is more
important than the first library guess.

## Unlock Boundary

Supported MVP policies:

- `passphrase`
- `key-and-passphrase`

Rules:

- no key-only unlock
- no silent unlock from SSH-agent alone
- no long-running daemon
- no cross-invocation unlock cache
- passphrase prompt must require an interactive TTY for real writes/unlocks
- tests may use explicit test-only passphrase injection through non-production
  helpers, not public CLI flags

`key-and-passphrase` may be staged as passphrase-only if SSH key wrapping needs
a later card, but it must not weaken into key-only behavior.

## Execution Chain

- `706` complete: open this lane and split implementation cards
- `707` complete: added secrets-domain crate and vault file model
- `708` complete: added vault crypto round trip
- `709` complete: added `secrets init/set/unset`
- `710` complete: added `secrets unlock/lock` and doctor vault diagnostics
- `711` complete: close `g05.003`

## Hard Boundaries

- no runtime secret injection
- no container startup injection
- no deploy/provider secret provisioning
- no compatibility `.env` export
- no `.env.schema` behavior removal
- no team sharing or cloud sync
- no release execution
- no `.github/workflows/` edits

## Acceptance

This lane is complete when a developer can initialize a local vault, set and
unset declared secret values, unlock explicitly with human participation, and
run doctor diagnostics that distinguish missing, locked, corrupt, unsafe, and
incomplete vault states without exposing values.

## Outcome

`g05.003` is complete. Effigy owns a local encrypted vault implementation and
value-free operator surface. Runtime injection remains deliberately deferred to
`g05.004`.

## Next Task

Open the first `g05.004` task/Rhai/deploy injection card.
