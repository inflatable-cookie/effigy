# Compatibility Env Export

Implemented batch card `718` for `g05.005`.

## Changed

- Added `effigy secrets export --format env --output <PATH> --yes`.
- Required explicit `--yes` for plaintext export.
- Rejected stdout export and repo-root `.env`.
- Exported declared vault values as dotenv-compatible `KEY=VALUE` lines.
- Blocked missing required secrets before writing.
- Kept text/JSON output value-free.
- Updated help, command reference, and JSON examples.

## Validation

- `cargo test secrets_option_tests`
- `cargo test secrets_export`

## Next

Close `g05.005` or continue to the Underlay/Acowtancy proof in `g05.006`.
