# 027 - Copy/Paste Snippets

Use this guide for quick manifest bootstraps you can paste and adapt.

For CI workflow snippets, use [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md).

## Start Here

Use this page when you want a working starting point faster than you want a
full explanation.

Pick the snippet by the repo shape in front of you:

- small single-language repo: start with `Single Rust Repo` or `JS App`
- multi-catalog workspace: use `Mixed Monorepo Root + Child Catalogs`
- local dev stack: use `Managed Dev Front Door`
- test ownership and env clarity: use `Built-in Test Suites` and `Cargo
  Isolation`
- repo-health checks: use `Repository Scanner Config`

Then verify the pasted shape with:

```sh
effigy tasks
effigy test --plan
```

## 1) Single Rust Repo (`effigy.toml`)

```toml
[catalog]
alias = "app"

[tasks]
fmt = "cargo fmt --all"
lint = "cargo clippy --all-targets --all-features -- -D warnings"
check = [{ task = "fmt" }, { task = "lint" }]
```

Run:

```sh
effigy check
effigy test --plan
```

This starter deliberately leaves `test` to the built-in runner.

## 2) JS App (`effigy.toml`)

```toml
[catalog]
alias = "web"

[package_manager]
js = "bun"

[tasks]
dev = "bun run dev"
lint = "bun run lint"
build = "bun run build"
validate = [{ task = "lint" }, { task = "build" }]
```

Run:

```sh
effigy validate
effigy test --plan
effigy watch --owner effigy --once test
```

This starter also leaves `test` on the built-in path so package-manager
autodetection stays available.

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

## 4) Managed Dev Front Door

```toml
[catalog]
alias = "app"

[systems]
default = "dev"

[systems.dev]
default_workspace = "app"

[systems.dev.workspaces.app]
container = "web"
workdir = "."

[tasks.dev]
mode = "tui"
fail_on_non_zero = true
workspace = "app"

[tasks.dev.managed]
container_lifecycle = true
health_wait = true
ready_message = "App ready at http://project.test"
gateway = true

concurrent = [
  { name = "app", role = "lifecycle", start = 1, tab = 1 },
  { task = "app/worker", start = 2, tab = 3, start_after_ms = 1200 },
  { run = "bun run web:dev", start = 3, tab = 2, shutdown_on_exit = true },
  { name = "terminal", role = "shell", start = 4, tab = 4 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { name = "app", role = "lifecycle", start = 1, tab = 1 },
  { run = "bun run admin:dev", start = 2, tab = 2 },
  { name = "terminal", role = "shell", start = 3, tab = 3 }
]
```

Run:

```sh
effigy dev
effigy dev admin
```

Use this when the repo wants one named task to own the local environment and
the tab runtime together. The fuller shipped contract lets `effigy dev`:

- resolve one repo-owned `workspace`
- attach that workspace to its backing container through `systems`
- wait on container health before showing readiness
- project one repo-owned ready message
- auto-start the shipped gateway when the container declares local domains
- open an embedded container shell with `role = "shell"`

Use `shutdown_on_exit = true` on the process that should act as the session
root when one app process should still stop the rest of the managed stack.

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

## 8) Repository Scanner Config

```toml
[scan.god_files]
warn = 250
high = 400
critical = 700
doctor = true
respect_gitignore = true
include = ["src/**", "app/**"]
exclude = ["docs/**", "dist/**", "coverage/**"]
```

Run:

```sh
effigy scan god-files
effigy scan god-files --show-warnings
effigy scan god-files --fail-on-findings
effigy scan god-files --markdown --out reports/god-files.md
effigy scan duplicate-blocks
effigy scan duplicate-blocks --show-warnings
effigy scan duplicate-blocks --fail-on-findings
effigy scan duplicate-blocks --markdown --out reports/duplicate-blocks.md
effigy scan comment-ratio
effigy scan comment-ratio --show-warnings
effigy scan comment-ratio --fail-on-findings
effigy scan comment-ratio --markdown --out reports/comment-ratio.md
effigy scan generated-assets
effigy scan generated-assets --show-warnings
effigy scan generated-assets --markdown --out reports/generated-assets.md
effigy scan generated-in-src
effigy scan generated-in-src --show-warnings
effigy scan generated-in-src --markdown --out reports/generated-in-src.md
effigy scan attention-markers
effigy scan attention-markers --show-warnings
effigy scan attention-markers --markdown --out reports/attention-markers.md
effigy scan stale-suppressions
effigy scan stale-suppressions --show-warnings
effigy scan stale-suppressions --markdown --out reports/stale-suppressions.md
```

Default text mode hides warning rows and prints a warning count summary. Use `--show-warnings` when you need the full terminal list.
Keep `[scan.stale_suppressions].doctor = false` unless you want suppression findings folded into `effigy doctor`.

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`028-migration-quick-paths.md`](./028-migration-quick-paths.md)

## Expected Outcome

After this guide, you should be able to:

- grab a working baseline for the repo shape you have
- adapt it without guessing the basic command surface
- move from snippet to the deeper cookbook only when you need more nuance

## Next Step

After pasting a snippet, move to
[`022-manifest-cookbook.md`](./022-manifest-cookbook.md) to turn the copied
baseline into an intentional repo-specific contract, then validate the result
with the checks in
[`029-docs-qa-checklist-and-validation.md`](./029-docs-qa-checklist-and-validation.md).
