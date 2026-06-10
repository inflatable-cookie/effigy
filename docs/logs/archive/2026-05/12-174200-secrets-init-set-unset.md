# Secrets Init Set Unset

Completed ready card `709`.

## Vision Target Delta

Effigy can now mutate the local encrypted vault:

- `effigy secrets init`
- `effigy secrets set <name>`
- `effigy secrets unset <name>`

The commands enforce declared secret names, write encrypted vault documents,
use hidden interactive input for real CLI operation, and keep text/JSON output
value-free.

Test-only input remains isolated behind `EFFIGY_TEST_SECRETS_PASSPHRASE` and
`EFFIGY_TEST_SECRETS_VALUE`; these are not public CLI flags.

## Validation

- `cargo test secrets_option_tests`
- `cargo test secrets_tests`

## Next Task

Execute `710` to add unlock/lock and doctor vault diagnostics.
