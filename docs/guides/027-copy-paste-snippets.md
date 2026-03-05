# 027 - Copy/Paste Snippets

Use this guide for quick manifest bootstraps you can paste and adapt.

For CI workflow snippets, use [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md).

## 1) Single Rust Repo (`effigy.toml`)

```toml
[catalog]
alias = "app"

[tasks]
fmt = "cargo fmt --all"
lint = "cargo clippy --all-targets --all-features -- -D warnings"
test = "cargo test"
check = [{ task = "fmt" }, { task = "lint" }, { task = "test" }]
```

Run:

```sh
effigy check
```

## 2) JS App (`effigy.toml`)

```toml
[catalog]
alias = "web"

[package_manager]
js = "bun"

[tasks]
dev = "bun run dev"
lint = "bun run lint"
test = "bun x vitest run"
build = "bun run build"
validate = [{ task = "lint" }, { task = "test" }, { task = "build" }]
```

Run:

```sh
effigy validate
effigy watch --owner effigy --once test
```

## 3) Mixed Monorepo Root + Child Catalogs

Root `effigy.toml`:

```toml
[catalog]
alias = "root"

[test]
max_parallel = 2

[tasks]
validate = [{ task = "api/validate" }, { task = "web/validate" }]
dev = "api/dev"
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

[package_manager]
js = "bun"

[tasks]
dev = "bun run dev"
validate = [{ run = "bun run lint" }, { run = "bun x vitest run" }]
```

Run:

```sh
effigy validate
effigy api/dev
effigy web/validate
```

## 4) Managed TUI Dev Stack

```toml
[catalog]
alias = "app"

[tasks.dev]
mode = "tui"
fail_on_non_zero = true

concurrent = [
  { task = "app/api", start = 1, tab = 2 },
  { task = "app/worker", start = 2, tab = 3, start_after_ms = 1200 },
  { run = "bun run web:dev", start = 3, tab = 1 },
  { task = "shell", start = 4, tab = 4 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { task = "app/api", start = 1, tab = 2 },
  { run = "bun run admin:dev", start = 2, tab = 1 }
]
```

Run:

```sh
effigy dev
effigy dev admin
```

## 5) Built-in Test Suites as Source of Truth

```toml
[package_manager]
js = "bun"

[test]
max_parallel = 3
cargo_env_match = "prefix-aware"

[test.suites]
unit = "bun x vitest run"
integration = "cargo nextest run --workspace"

[test.runners]
vitest = "bun x vitest run"
"cargo-nextest" = "cargo nextest run --workspace"
"cargo-test" = "cargo test --workspace"
```

Run:

```sh
effigy test --plan
effigy test unit
effigy test integration
```

## 6) Cargo Isolation Per Task

```toml
[catalog]
alias = "api"

[env]
cargo = [
  { CARGO_HOME = "{project}/.effigy/cargo/home" },
  { CARGO_TARGET_DIR = "{project}/.effigy/cargo/target" }
]

[tasks]
build = [{ env = "cargo" }, { run = "cargo build --workspace" }]
check = [
  { env = "cargo" },
  { run = "cargo check --workspace" }
]
```

Run:

```sh
effigy build
effigy check
```

Use when several repos build at the same time and you need project-local Cargo directories.
You can also reference named env values from another catalog root with `env = "../shared/CARGO_HOME"`.

If `env = "<NAME>"` is not found in `[env]` or the process environment, Effigy falls back to dotenv files.
By default it reads `.env`; override per-task or mid-chain with `env_file`:

```toml
[tasks.test]
env_file = ".env.test"
run = [{ env = "DATABASE_URL" }, { run = "cargo test --workspace" }]

[tasks.migrate]
run = [
  { env_file = [".env.local", ".env.test"] },
  { env = "DATABASE_URL" },
  { task = "db:migrate" }
]
```

## 7) Deferral Compatibility Snippet

```toml
[defer]
run = "composer global exec effigy -- {request} {args}"
```

Use only when unresolved selectors must forward to legacy tooling.

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)

## Next Step

After pasting a snippet, validate routing and health with the checklist in [`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md).
