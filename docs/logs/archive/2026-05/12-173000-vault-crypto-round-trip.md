# Vault Crypto Round Trip

Completed ready card `708`.

## Vision Target Delta

`effigy-secrets` now encrypts and decrypts vault payloads.

Added:

- Argon2id passphrase-derived vault key
- XChaCha20-Poly1305 authenticated encryption
- OS-random salt and nonce generation
- deterministic test-only material path for fixtures
- wrong-passphrase and corrupt-payload fail-closed behavior

No CLI mutation, prompts, unlock cache, runtime injection, provider secret
provisioning, or `.env.schema` behavior changes were added.

## Validation

- `cargo test -p effigy-secrets`

## Next Task

Execute `709` to add `secrets init`, `set`, and `unset`.
