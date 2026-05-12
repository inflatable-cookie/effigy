# Task Secret Injection

Implemented batch card `713` for `g05.004`.

## Changed

- Resolved `[secrets.keys.*]` declarations with `targets = ["tasks"]` from the
  local Effigy vault before task execution.
- Injected task secrets through the existing process environment path shared
  with `.env.schema` sensitive values.
- Blocked missing required task secrets before spawning host or container task
  commands.
- Redacted known secret values from captured task JSON output for host,
  routed-container, and inline workspace task execution.
- Added tests for task vault injection, missing required blockers, and existing
  task env schema compatibility.

## Validation

- `cargo test run_manifest_task_injects_declared_vault_secret_into_env`
- `cargo test run_manifest_task_blocks_missing_required_vault_secret_before_spawn`
- `cargo test task_env`
- `cargo check --all-targets`

## Next

Execute `714` to add the Rhai secret API.
