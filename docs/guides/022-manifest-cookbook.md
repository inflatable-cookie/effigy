# 022 - Manifest Cookbook (`effigy.toml` Patterns)

This cookbook provides copy-paste manifest patterns you can adapt directly.

## 1) Small Single-Repo Starter

```toml
[catalog]
alias = "app"

[tasks]
fmt = "cargo fmt --all"
lint = "cargo clippy --all-targets --all-features -- -D warnings"
test = "cargo test"
```

Use when you want one local catalog with explicit command ownership.

## 2) Compact Tasks + Task Chain

```toml
[tasks]
api = "cargo run -p api {args}"
worker = "cargo run -p worker {args}"
"db:reset" = [{ task = "db:drop" }, { task = "db:migrate" }]
"db:drop" = "sqlx database drop -y"
"db:migrate" = "sqlx migrate run"
```

Use compact syntax for straightforward run commands and lightweight chains.

## 3) Full Task Table with Runtime Controls

```toml
[tasks.build]
run = "bun run build"
fail_on_non_zero = true
```

Use full task tables when you need settings (`fail_on_non_zero`, `env`, `mode`, `profiles`, etc.).

## 4) DAG-Style Validation Flow

```toml
[tasks.validate]
run = [
  { id = "lint", run = "bun run lint", retry = 1, retry_delay_ms = 200 },
  { id = "unit", task = "test vitest", depends_on = ["lint"], timeout_ms = 180000 },
  { id = "contract", run = "./scripts/check-json-contracts.sh --fast", depends_on = ["lint"] },
  { id = "report", run = "printf validate-ok", depends_on = ["unit", "contract"], fail_fast = false }
]
```

Use when you need dependency-aware orchestration, retry policy, and per-step timeouts.

## 4b) Run-Array Env Directives

```toml
[env]
CARGO_HOME = "{project}/.effigy/cargo/home"
CARGO_TARGET_DIR = "{project}/.effigy/cargo/target"
# Optional grouped profile form:
cargo = [{ CARGO_HOME = "{project}/.effigy/cargo/home" }, { CARGO_TARGET_DIR = "{project}/.effigy/cargo/target" }]

[tasks.api]
env_file = [".env.local", ".env.test"]
run = [
  { env = "cargo" },
  { env = "DATABASE_URL" },
  { run = "cargo run -p api {args}" },
  { env = { RUST_LOG = "debug" } },
  { task = "jobs" }
]
```

Use this when you want env changes to take effect at specific points in a run chain.

Behavior:
- an `env` step updates the effective env for subsequent run-array entries
- an `env_file` step updates dotenv fallback sources for subsequent run-array entries
- `env = "<name>"` resolves in order: top-level `[env]`, process env, then `<catalog-root>/.env`
- `env = "<catalog-path>/<name>"` resolves `<name>` from another catalog `[env]` table, then that catalog `.env` (relative to current catalog root unless absolute)
- cross-catalog refs (`env = "<catalog-path>/<name>"`) do not use process env fallback
- `[env].<name>` can be either a single value (`KEY = "value"`) or a grouped profile array (`name = [{ KEY = "value" }, ...]`)
- `tasks.<name>.env_file` sets fallback dotenv for that task; accepts string or ordered array (`[".env.local", ".env.test"]`)
- run arrays can update fallback dotenv mid-chain with `{ env_file = ".env.test" }` or `{ env_file = [".env.local", ".env.test"] }`
- for array form, files are checked in order and first file containing the key wins
- `env` and `env_file` steps can be mixed with `run` and `task` steps
- `env`/`env_file` directives can be standalone entries with no `run`/`task` command (no-op step used for state changes)
- `tasks.<name>.env` still applies globally to the whole task; run-array `env` steps can override later entries
- `{project}`/`{repo}` in env values always resolve from the task currently executing
- `.env` parsing accepts `KEY=value` and `export KEY=value`; matching single/double quotes are stripped from values

## 5) Managed Dev Stack (`mode = "tui"`)

```toml
[tasks.dev]
mode = "tui"
shell = true
fail_on_non_zero = true

concurrent = [
  { task = "app/api", start = 1, tab = 2 },
  { task = "app/worker", start = 2, tab = 3, start_after_ms = 1200 },
  { run = "bun run docs:dev", start = 3, tab = 1 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { task = "app/api", start = 1, tab = 2 },
  { run = "bun run admin:dev", start = 2, tab = 1 }
]
```

Use for multi-process local development with profile-specific variants.

## 6) Built-in Test Fanout and Suite Source of Truth

```toml
[package_manager]
js = "bun"

[test]
max_parallel = 2

[test.suites]
unit = "bun x vitest run"
integration = "cargo nextest run --workspace"

[test.runners]
vitest = "bun x vitest run"
"cargo-nextest" = "cargo nextest run --workspace"
"cargo-test" = "cargo test --workspace"
```

Use this to make test routing explicit and reproducible across mixed stacks.

## 7) Minimal Test Runner Override Only

```toml
[test.runners]
vitest = "bun x vitest run"
"cargo-nextest" = { command = "cargo nextest run --workspace --status-level skip" }
```

Use when auto-detection is fine but default commands need tuning.

## 8) Deferral Fallback for Legacy Interop

```toml
[defer]
run = "my-process {request} {args}"
```

Use only when unresolved selectors should be delegated to another runner.

## 9) Shell Override for Managed Tabs

```toml
[shell]
run = "exec ${SHELL:-/bin/zsh} -i"
```

Use when you need predictable interactive shell startup behavior in TUI shell tabs.

## 10) Explicit Task Cache (Phase 1)

```toml
[tasks.build]
run = "cargo build --workspace"

[tasks.build.cache]
enabled = true
inputs = ["src/**/*.rs", "Cargo.toml", "Cargo.lock"]
outputs = ["target/debug/my-app"]
env = ["RUSTFLAGS", "CARGO_PROFILE_DEV_DEBUG"]
```

Use this for deterministic local up-to-date checks.

Phase-1 guardrails:
- cache is opt-in only (`enabled = true`)
- no implicit input/output discovery
- cache hit requires matching fingerprint and declared outputs to exist

Inspection and invalidation:
- `effigy cache inspect build`
- `effigy cache invalidate build`
- `effigy cache invalidate --all`

## 11) Task-Local Runtime Env (Cargo Isolation)

Compact inline-table shape:

```toml
[env]
CARGO_HOME = "{project}/.effigy/cargo/home"
CARGO_TARGET_DIR = "{project}/.effigy/cargo/target"

[tasks]
build = [{ env = "CARGO_HOME" }, { env = "CARGO_TARGET_DIR" }, { run = "cargo build --workspace" }]
```

Full task-table shape:

```toml
[tasks.build]
run = "cargo build --workspace"
env = { CARGO_HOME = "{project}/.effigy/cargo/home", CARGO_TARGET_DIR = "{project}/.effigy/cargo/target" }
```

Use this when multiple repos build concurrently and you need project-local Cargo state to avoid cross-repo contention.

Behavior:
- process environment is inherited by default
- `tasks.<name>.env` overrides inherited values for that task
- run-array env directives support either inline maps (`env = { ... }`) or named entries (`env = "CARGO_HOME"`/`env = "cargo"` from `[env]`)
- named entry resolution order is `[env]` -> process env -> dotenv fallback (`.env` or `env_file` override)
- referenced tasks keep their own `env` when called via `task = "..."` entries
- env value token substitution supports `{project}` and `{repo}` (aliases for catalog root path)
- built-in `test` also reads manifest `[env]` for `CARGO_*` keys and applies them automatically to cargo suites (`cargo-nextest`/`cargo-test`)

## 12) Multi-Catalog Monorepo Baseline

Root `effigy.toml`:

```toml
[catalog]
alias = "root"

[tasks]
validate = [{ task = "api/validate" }, { task = "web/validate" }]
```

`services/api/effigy.toml`:

```toml
[catalog]
alias = "api"

[tasks]
dev = "cargo run -p api"
validate = [{ run = "cargo fmt --check" }, { run = "cargo test" }]
```

`apps/web/effigy.toml`:

```toml
[catalog]
alias = "web"

[tasks]
dev = "bun run dev"
validate = [{ run = "bun run lint" }, { run = "bun run test" }]
```

Use catalog aliases to keep task ownership local while retaining root-level orchestration.

## Notes

- Discovery scans for `effigy.toml` recursively.
- Catalog aliases must be unique across discovered manifests.
- Useful interpolation tokens in run commands:
  - `{project}` catalog root path (shell-quoted alias of `{repo}`)
  - `{repo}` catalog root path (shell-quoted)
  - `{args}` passthrough args (shell-quoted)
  - `{request}` unresolved selector (deferral only)
- Useful interpolation tokens in `tasks.<name>.env` values and `[env]` entries:
  - `{project}` catalog root path
  - `{repo}` alias of `{project}`

## Related Guides

- [`013-testing-orchestration.md`](./013-testing-orchestration.md)
- [`015-deferral-fallback-migration.md`](./015-deferral-fallback-migration.md)
- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md)
- [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
