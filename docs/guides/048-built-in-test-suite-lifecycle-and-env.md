# 048 - Built-in Test Suite Lifecycle and Env

This guide explains the full builtin `[test.suites.<name>]` contract for managed test environments.

Use it when you want `effigy test` to remain the entrypoint while a suite also needs:
- suite-specific `env` / `env_file`
- setup before the runner starts
- teardown after the runner exits
- guaranteed cleanup with `teardown_policy = "always"`

## Vision Alignment

- Primary tags: `OPERATE`, `ROUTE`
- Target movement: managed test environments stay declarative inside builtin `effigy test` instead of drifting into wrapper scripts.

## 1) When to Use a Lifecycle-Aware Suite

Use plain string suites when you only need a stable command:

```toml
[test.suites]
unit = "bun x vitest run"
integration = "cargo nextest run --workspace"
```

Use a full suite table when the test runner needs managed state around it:

```toml
[test.suites.managed]
run = "cargo nextest run --workspace --test-threads=1 --build-jobs=1"
env = "TEST_DATABASE_URL"
env_file = ".env"
setup = [
  { run = "cargo run -p app-db --bin reset_test_db" },
  { run = "cargo run -p app-db --bin migrate_test_db" },
]
teardown = [
  { run = "cargo run -p app-db --bin reset_test_db" },
]
teardown_policy = "always"
```

Use this shape for database-backed integration suites, fixture bootstrapping, cache warmup, or any case where the suite command alone is not enough.

## 2) Contract Summary

Supported fields on `[test.suites.<name>]`:

- `run`
  - required test runner command
- `env`
  - optional named env entry, grouped profile, or inline env map using the same resolution model as managed tasks
- `env_file`
  - optional dotenv source override; accepts string or ordered array
- `setup`
  - optional run-step array executed before the suite command
- `teardown`
  - optional run-step array executed after the suite command
- `teardown_policy`
  - optional; `on-success` or `always`
  - default is `on-success`

The suite still runs through builtin `effigy test`; this is not a separate task system.

## 3) Execution Order

Lifecycle-aware suites execute in this order:

1. resolve suite `env` / `env_file`
2. run `setup` steps in order
3. run the suite `run` command with any passthrough runner args
4. run `teardown` based on `teardown_policy`

Behavior notes:

- resolved suite env applies to `setup`, `run`, and `teardown`
- if setup fails, the suite runner command is skipped
- with `teardown_policy = "always"`, teardown still runs after setup failure or runner failure
- with default `on-success`, teardown only runs after a successful suite command

## 4) Env Resolution

Suite `env` and `env_file` reuse the same model as managed tasks.

Resolution order for named env entries:

1. top-level `[env]` entry in the selected catalog
2. process environment
3. dotenv fallback (`.env` by default, or `env_file` override)

Examples:

```toml
[env]
TEST_DATABASE_URL = "postgres://localhost/app-test"
managed-rust = [
  { CARGO_HOME = "{project}/.effigy/cargo/home" },
  { CARGO_TARGET_DIR = "{project}/.effigy/cargo/target" },
]

[test.suites.managed]
run = "cargo nextest run --workspace"
env = "managed-rust"
env_file = [".env.local", ".env.test"]
```

This keeps suite-local env behavior aligned with the rest of the manifest instead of creating a second env system just for tests.

## 5) Nextest Patterns

Typical nextest examples:

```toml
[test.suites]
unit = "cargo nextest run --workspace --profile default"

[test.suites.integration]
run = "cargo nextest run --workspace --test-threads=1 --build-jobs=1"
env = "TEST_DATABASE_URL"
env_file = ".env"
setup = [
  { run = "cargo run -p app-db --bin reset_test_db" },
  { run = "cargo run -p app-db --bin migrate_test_db" },
]
teardown = [
  { run = "cargo run -p app-db --bin reset_test_db" },
]
teardown_policy = "always"
```

This pattern is the intended replacement for custom wrapper tasks that only exist to reset/migrate test state around `cargo nextest run`.

## 6) Passthrough Args and `--`

Builtin `effigy test` still owns suite selection. Use `--` when the remaining arguments belong to the underlying runner rather than Effigy.

Examples:

```sh
effigy test managed -- --package catalog_a-db --test learning_soft_delete
effigy test vitest -- --runInBand
effigy test nextest -- user_service --nocapture
```

Rules:

- the optional suite name comes before `--`
- everything after `--` is forwarded to the suite command
- use `effigy test --plan` first if you are unsure whether a token will be treated as a suite name or passthrough runner arg

## 7) Plan and Results Output

Use plan mode to inspect the lifecycle contract without running tests:

```sh
effigy test --plan
```

Configured suites now report lifecycle metadata such as:

- `suite-env`
- `suite-env-files`
- `setup-steps`
- `teardown-steps`
- `teardown-policy`

This makes managed suites inspectable from CLI output instead of requiring users to read `effigy.toml`.

## 8) Failure Patterns

Symptom: setup ran but cleanup did not run after test failure.

Fix:
- set `teardown_policy = "always"` for suites that must always clean up shared state

Symptom: runner filters or flags are being interpreted as Effigy arguments.

Fix:
- place the suite name first, then use `--` before runner-specific arguments

Symptom: suite env values are not present during setup or teardown.

Fix:
- verify `env` / `env_file` on the suite with `effigy test --plan`
- confirm the env entry exists in `[env]` or the selected dotenv file

## Expected Outcome

After applying this pattern, `effigy test` remains the single entrypoint for test orchestration while complex suites can still manage env, setup, and guaranteed teardown declaratively.

## Related Guides

- [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

## Next Step

If you are migrating off wrapper scripts, update the target repo manifest first, then confirm `suite-env`, `setup-steps`, and `teardown-policy` with `effigy test --plan` before replacing existing CI or developer entrypoints.
