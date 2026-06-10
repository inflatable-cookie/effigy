# Secret Domain Vault File Model

Completed ready card `707`.

## Vision Target Delta

Effigy now has a dedicated `effigy-secrets` domain crate for the local vault
model.

Added:

- versioned vault envelope model
- placeholder encrypted payload model
- redacted `SecretValue` wrapper
- plaintext vault record model for later encryption
- vault model validation errors
- Unix file permission diagnostics

No encryption, prompts, mutation commands, unlock cache, or runtime injection
was added.

## Validation

- `cargo test -p effigy-secrets`

## Next Task

Execute `708` to add vault crypto round-trip support.
