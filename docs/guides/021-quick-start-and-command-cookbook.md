# 021 - Quick Start and Command Cookbook

This guide is a practical operator walkthrough for all current built-in Effigy features.

## 1) Quick Setup

Run from source:

```sh
cargo run --bin effigy -- --help
cargo run --bin effigy -- tasks
```

Install on PATH:

```sh
cargo install --path .
effigy --help
```

Create starter manifest if needed:

```sh
effigy init
effigy tasks
```

## 2) Minimal `effigy.toml`

```toml
[catalog]
alias = "app"

[tasks]
dev = "bun run dev"
api = "cargo run -p api"
"db:reset" = "./scripts/reset-db.sh"
```

Run:

```sh
effigy dev
effigy app/db:reset
```

## 3) Task Discovery and Routing

List catalogs and tasks:

```sh
effigy tasks
effigy tasks --task test
```

Probe selector routing evidence:

```sh
effigy tasks --resolve test
effigy tasks --resolve app/db:reset
```

Machine mode:

```sh
effigy --json tasks --resolve test
```

See also: [`016-task-routing-precedence.md`](./016-task-routing-precedence.md).

## 4) Doctor Health and Explain Mode

Workspace health checks:

```sh
effigy doctor
effigy doctor --fix
effigy doctor --verbose
```

Explain selection for a specific invocation:

```sh
effigy doctor --repo /path/to/workspace app/build -- --watch
```

Machine mode:

```sh
effigy --json doctor
effigy --json doctor --repo /path/to/workspace app/build -- --watch
```

See also: [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md).

## 5) Built-in Test Orchestration

Run detected suite(s):

```sh
effigy test
```

Plan without executing:

```sh
effigy test --plan
effigy --json test --plan
```

Choose explicit suite in mixed repos:

```sh
effigy test vitest
effigy test nextest user_service --nocapture
```

Useful options:

```sh
effigy test --verbose-results
effigy test --tui
effigy test -- --runInBand
```

See also: [`013-testing-orchestration.md`](./013-testing-orchestration.md), [`022-manifest-cookbook.md`](./022-manifest-cookbook.md), and [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md).

## 6) Watch Mode (Phase 1)

Bounded run (CI-safe):

```sh
effigy watch --owner effigy --once test
effigy watch --owner effigy --max-runs 2 --json test vitest
```

Long-running loop:

```sh
effigy watch --owner effigy --debounce-ms 500 --include "src/**" --exclude "**/*.snap" test vitest
```

Notes:
- `--owner` is required.
- `--json` requires `--once` or `--max-runs`.
- default excludes: `.git/**`, `node_modules/**`, `target/**`.

See also: [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md).

## 7) Init and Migrate

Init scaffolding:

```sh
effigy init --dry-run
effigy init --force
```

Migrate scripts from `package.json`:

```sh
effigy migrate
effigy migrate --script build --script test
effigy migrate --apply
effigy migrate --from ./frontend/package.json --apply --json
```

## 8) Config Reference Generator

Human-readable configuration reference:

```sh
effigy config
effigy --json config
```

Schema snippets:

```sh
effigy config --schema
effigy config --schema --minimal
effigy config --schema --target test
effigy config --schema --target test --runner vitest
```

Supported `--target` values:
- `package_manager`
- `test`
- `tasks`
- `defer`
- `shell`

## 9) Lock Recovery (`unlock`)

Unlock one scope:

```sh
effigy unlock workspace
effigy unlock task:watch:test
effigy unlock profile:dev/admin
```

Unlock all scopes:

```sh
effigy unlock --all
effigy unlock --all --json
```

Use this when a prior interrupted process leaves lock files behind.

## 10) Deferral Fallback

When no task matches, optional deferral can forward requests:

```toml
[defer]
run = "my-process {request} {args}"
```

See: [`015-deferral-fallback-migration.md`](./015-deferral-fallback-migration.md).

## 11) JSON Contract Summary

Canonical machine mode:

```sh
effigy --json <command>
```

Examples:

```sh
effigy --json help
effigy --json tasks
effigy --json doctor
effigy --json test --plan
effigy --json watch --owner effigy --once test
```

Envelope schema: `effigy.command.v1`.
Payload schemas are command-specific and documented in `017-json-output-contracts.md`.

## Next Reading

- Manifest authoring patterns: [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- Troubleshooting by symptom: [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- CI and automation recipes: [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
