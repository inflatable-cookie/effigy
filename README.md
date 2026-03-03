# effigy

Effigy is a unified task runner for monorepos and nested workspaces.

It gives you one command surface for:
- project tasks from `effigy.toml`,
- built-in workflow commands (`tasks`, `doctor`, `test`, `watch`, `init`, `migrate`, `config`, `unlock`, `cache`, `completion`),
- deterministic task resolution across catalogs.

## Quick Start (2 Minutes)

1. Build and run help:

```bash
cargo run --bin effigy -- --help
```

2. Preview your discovered tasks:

```bash
cargo run --bin effigy -- tasks
```

3. Scaffold a starter manifest if you do not have one:

```bash
cargo run --bin effigy -- init
```

4. Add a minimal task catalog in `effigy.toml`:

```toml
[catalog]
alias = "app"

[tasks]
dev = "bun run dev"
test = "bun x vitest run"
"db:reset" = "./scripts/reset-db.sh"
```

5. Run tasks:

```bash
cargo run --bin effigy -- dev
cargo run --bin effigy -- app/db:reset
```

## Most Common Commands

```bash
effigy tasks
effigy tasks --resolve test
effigy doctor --verbose
effigy test --plan
effigy watch --owner effigy --once test
effigy --json tasks
effigy unlock --all
effigy cache inspect
effigy cache invalidate build
effigy completion zsh > ~/.zfunc/_effigy
effigy completion candidates --prefix app
```

## Contributor Commands

```bash
cargo qa
cargo qa-docs
cargo qa-json
cargo qa-json-ci
cargo qa-release
```

Fallback:

```bash
./scripts/check-quality-gates.sh
./scripts/check-quality-gates.sh --docs-only
./scripts/check-quality-gates.sh --json-only
./scripts/check-quality-gates.sh --json-only --ci
./scripts/check-release-gates.sh
./scripts/check-release-install-from-tag.sh --tag v0.__.__
```

## Task Catalog Basics

Manifest name: `effigy.toml` (discovered recursively).

Example:

```toml
[catalog]
alias = "catalog-a"

[tasks."db:reset"]
run = "cargo run -p app-db --bin reset_dev_db {args}"

[env]
CARGO_HOME = "{project}/.effigy/cargo/home"
CARGO_TARGET_DIR = "{project}/.effigy/cargo/target"
cargo = [{ CARGO_HOME = "{project}/.effigy/cargo/home" }, { CARGO_TARGET_DIR = "{project}/.effigy/cargo/target" }]

[tasks.api]
run = [{ env = "cargo" }, { env = { RUST_LOG = "info" } }, { run = "cargo run -p api {args}" }]
# or pull a named env value from another catalog root:
# run = [{ env = "../shared/CARGO_HOME" }, { run = "cargo run -p api" }]

[tasks.test]
env_file = [".env.local", ".env.test"]
run = [{ env = "DATABASE_URL" }, { run = "cargo test -p api" }]
# run arrays can switch dotenv source mid-chain:
# run = [{ env_file = ".env.test" }, { env = "DATABASE_URL" }, { run = "cargo test -p api" }]

[tasks.build.cache]
enabled = true
inputs = ["src/**/*.rs", "Cargo.toml"]
outputs = ["target/app"]
env = ["RUSTFLAGS", "NODE_ENV"]
```

Interpolation tokens:
- `{project}`: resolved catalog root path (alias of `{repo}`)
- `{repo}`: resolved catalog root (shell-quoted)
- `{args}`: passthrough args (shell-quoted)
- `{request}`: original unresolved selector (deferral only)
- in `tasks.<name>.env` values and `[env]` entries, `{project}`/`{repo}` resolve to the catalog root path
- run-array `env = "<name>"` resolves in order: top-level `[env]`, process env, then `<catalog-root>/.env`
- run-array `env = "<catalog-path>/<name>"` resolves `<name>` from that catalog's `[env]`, then that catalog's `.env` (path is relative to current catalog unless absolute)
- run-array `env`/`env_file` directives may be standalone entries (no `run`/`task`) to mutate env state for later steps
- set `tasks.<name>.env_file = ".env.test"` (or `[".env.local", ".env.test"]`) to change dotenv fallback for that task; run arrays can also switch with `{ env_file = ".env.local" }` or `{ env_file = [".env.local", ".env.test"] }`
- dotenv parsing accepts `KEY=value` and `export KEY=value` lines; matching single/double quotes around values are stripped
- built-in `test` auto-applies manifest `[env]` `CARGO_*` entries to cargo-based suites (`cargo-nextest` / `cargo-test`), so you do not need a custom `tasks.test` wrapper for Cargo home/target isolation

## Resolution Model

Root selection:
1. explicit `--repo <PATH>` when provided,
2. otherwise nearest marker root from cwd,
3. optional promotion to parent workspace when membership signals indicate it.

Task selection:
1. explicit prefix (`catalog/task`) selects one catalog,
2. unprefixed selector chooses nearest in-scope catalog,
3. otherwise shallowest from workspace root,
4. ties fail as explicit ambiguity.

Detailed routing guide: [`docs/guides/016-task-routing-precedence.md`](./docs/guides/016-task-routing-precedence.md).

## JSON Output

Canonical JSON mode:

```bash
effigy --json <command>
```

Top-level envelope schema: `effigy.command.v1`.

See [`docs/guides/017-json-output-contracts.md`](./docs/guides/017-json-output-contracts.md).

## Terminology Canon

Use the canonical terms across docs and PRs:
- `selector`: a task request string such as `test` or `api/test`
- `routing`: how a selector resolves to a catalog and task
- `deferral`: fallback execution when no selector matches local tasks

See [`docs/guides/034-task-and-command-glossary.md`](./docs/guides/034-task-and-command-glossary.md).

## Documentation Entry Points

- Docs system index: [`docs/README.md`](./docs/README.md)
- Guides landing (persona/task navigation): [`docs/guides/README.md`](./docs/guides/README.md)
- Contributor onboarding (15 min): [`docs/guides/030-contributor-onboarding-15-minutes.md`](./docs/guides/030-contributor-onboarding-15-minutes.md)
- Documentation contribution playbook: [`docs/guides/037-documentation-contribution-playbook.md`](./docs/guides/037-documentation-contribution-playbook.md)

## Development

```bash
cargo test
cargo qa
```

## Repository Layout

```text
effigy/
├── src/
├── docs/
│   ├── architecture/
│   ├── contracts/
│   ├── guides/
│   ├── roadmap/
│   └── reports/
└── Cargo.toml
```
