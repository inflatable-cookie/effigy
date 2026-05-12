# Rhai Secret API

Implemented batch card `714` for `g05.004`.

## Changed

- Added `effigy::secret(name)` and `effigy::has_secret(name)` for declared
  Rhai-targeted vault secrets.
- Added Rhai secret preflight so required missing secrets block before script
  execution.
- Rejected undeclared and wrong-target secret reads.
- Redacted known secret values from Rhai errors, host logs, process result
  maps, container exec result maps, and Effigy callback maps.

## Validation

- `cargo test -p effigy-rhai rhai_secret`
- `cargo test -p effigy-rhai execute_rhai_script_rejects_undeclared_and_wrong_target_secret_reads`
- `cargo test -p effigy-rhai execute_rhai_script_redacts_secret_values_from_errors`
- `cargo check --all-targets`

Full `cargo test -p effigy-rhai` now passes all Rhai secret-related tests but
still fails three unrelated existing Rhai surface/first-party script drift
checks involving bundle helper shape and external provider scripts.

## Next

Execute `715` to add deploy/state/artifact secret injection.
