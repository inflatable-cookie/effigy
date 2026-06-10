# Secrets Unlock Lock Doctor Vault

Completed ready card `710`.

## Vision Target Delta

Effigy now has the full local-vault command set for `g05.003`:

- `effigy secrets unlock`
- `effigy secrets lock`
- richer `effigy secrets doctor` vault diagnostics

Doctor now distinguishes missing, locked, unlocked, corrupt,
unsafe-permission, missing-required, and undeclared-stored vault states without
printing values.

No daemon, persistent unlock cache, runtime injection, provider provisioning,
or `.env.schema` behavior change was added.

## Validation

- `cargo test secrets_option_tests`
- `cargo test secrets_tests`

## Next Task

Execute `711` to document and close `g05.003`.
