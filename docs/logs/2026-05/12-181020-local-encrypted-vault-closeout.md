# Local Encrypted Vault Closeout

Completed ready card `711` and closed `g05.003`.

## Vision Target Delta

Effigy now has a built-in local encrypted vault for developer secrets:

- `effigy secrets init`
- `effigy secrets set <name>`
- `effigy secrets unset <name>`
- `effigy secrets unlock`
- `effigy secrets lock`
- vault-aware `effigy secrets doctor`

The implementation uses a dedicated `effigy-secrets` crate, Argon2id,
XChaCha20-Poly1305, hidden operator input, declared-key enforcement, safe Unix
file permissions, and value-free reports.

Runtime injection remains deferred to `g05.004`.

## Validation

- `cargo test secrets_option_tests`
- `cargo test secrets_tests`
- `cargo test -p effigy-secrets`
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- docs checks
- `git diff --check`

## Next Task

Execute `712` to open the task/Rhai/deploy secret injection lane.
