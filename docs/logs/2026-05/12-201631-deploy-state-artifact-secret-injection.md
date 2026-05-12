# Deploy State Artifact Secret Injection

Implemented batch card `715` for `g05.004`.

## Changed

- Added target-scoped Rhai secret execution so internal callers can opt into
  `deploy`, `state`, or `artifacts` secret access.
- Ran deploy provider package scripts with `deploy` secret scope.
- Added execution-request secret targets for task execution.
- Injected declared `state` secrets into state apply hook task environments.
- Kept default task secret behavior scoped to `targets = ["tasks"]`.

## Validation

- `cargo test state_apply_hook_receives_declared_state_secret`
- `cargo test -p effigy-rhai execute_rhai_script_can_use_deploy_target_secret_when_allowed`
- `cargo check --all-targets`

## Note

Artifact secret scope is available to internal Rhai workflow callers, but the
current built-in artifact stage/capture commands do not execute Rhai scripts.
No artifact-specific caller was added in this batch.

## Next

Execute `716` to close `g05.004`.
