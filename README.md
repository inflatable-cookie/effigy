# effigy

Effigy is a unified task runner for monorepos and nested workspaces.

It gives you one command surface for:
- project tasks from `effigy.toml`,
- built-in workflow commands (`tasks`, `doctor`, `test`, `watch`, `init`, `migrate`, `config`, `unlock`),
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

## Installation Options

Development invocation:

```bash
cargo run --manifest-path /abs/path/to/effigy/Cargo.toml --bin effigy -- tasks
```

PATH install (daily use):

```bash
cargo install --path .
effigy tasks
```

For PATH/release workflow details, see [`docs/guides/010-path-installation-and-release.md`](./docs/guides/010-path-installation-and-release.md).

## Most Common Commands

```bash
effigy tasks
effigy tasks --resolve test
effigy doctor
effigy doctor --fix
effigy test
effigy test --plan
effigy watch --owner effigy --once test
effigy config
effigy config --schema --target test
effigy migrate --apply
effigy unlock --all
```

## Task Catalog Basics

Manifest name: `effigy.toml` (discovered recursively).

Example:

```toml
[catalog]
alias = "catalog-a"

[tasks."db:reset"]
run = "cargo run -p app-db --bin reset_dev_db {args}"
```

Compact task syntax is also supported:

```toml
[tasks]
api = "cargo run -p app-api {args}"
jobs = "cargo run -p app-jobs {args}"
"db:reset" = [{ task = "db:drop" }, { task = "db:migrate" }]
```

Interpolation tokens:
- `{repo}`: resolved catalog root (shell-quoted)
- `{args}`: passthrough args (shell-quoted)
- `{request}`: original unresolved selector (deferral only)

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

Canonical machine mode:

```bash
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

- Top-level envelope schema: `effigy.command.v1`
- Payload schemas are command-specific (`effigy.tasks.v1`, `effigy.doctor.v1`, etc.)

See [`docs/guides/017-json-output-contracts.md`](./docs/guides/017-json-output-contracts.md).

## Extended Guides

Start with:
- Docs index: [`docs/README.md`](./docs/README.md)
- Guides landing page: [`docs/guides/README.md`](./docs/guides/README.md)
- Docs flow map: [`docs/guides/028-docs-flow-map.md`](./docs/guides/028-docs-flow-map.md)
- Command cookbook: [`docs/guides/021-quick-start-and-command-cookbook.md`](./docs/guides/021-quick-start-and-command-cookbook.md)
- Manifest cookbook: [`docs/guides/022-manifest-cookbook.md`](./docs/guides/022-manifest-cookbook.md)
- Testing orchestration: [`docs/guides/013-testing-orchestration.md`](./docs/guides/013-testing-orchestration.md)
- Watch/init/migrate: [`docs/guides/019-watch-init-migrate-phase-1.md`](./docs/guides/019-watch-init-migrate-phase-1.md)
- DAG + locks: [`docs/guides/020-dag-lock-policy-baseline.md`](./docs/guides/020-dag-lock-policy-baseline.md)
- Deferral migration: [`docs/guides/015-deferral-fallback-migration.md`](./docs/guides/015-deferral-fallback-migration.md)
- Troubleshooting recipes: [`docs/guides/023-troubleshooting-and-failure-recipes.md`](./docs/guides/023-troubleshooting-and-failure-recipes.md)
- CI and automation recipes: [`docs/guides/024-ci-and-automation-recipes.md`](./docs/guides/024-ci-and-automation-recipes.md)
- Command reference matrix: [`docs/guides/025-command-reference-matrix.md`](./docs/guides/025-command-reference-matrix.md)
- Copy/paste snippets: [`docs/guides/027-copy-paste-snippets.md`](./docs/guides/027-copy-paste-snippets.md)
- Migration quick paths: [`docs/guides/028-migration-quick-paths.md`](./docs/guides/028-migration-quick-paths.md)
- Recent release note (DAG/watch/onboarding): [`docs/reports/2026-02-28-dag-watch-onboarding-release-note.md`](./docs/reports/2026-02-28-dag-watch-onboarding-release-note.md)

## Development

Run tests:

```bash
cargo test
./scripts/check-doc-links.sh README.md $(find docs -name '*.md' | sort)
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
