# 025 - Command Reference Matrix

This matrix is a quick operator reference for Effigy commands, key flags, JSON payload schemas, and deep-dive guides.

## 1) Primary Commands

| Command | Purpose | Key Flags | JSON Schema(s) | Deep Dive |
| --- | --- | --- | --- | --- |
| `effigy help` / `effigy --help` | Show CLI help and topic guidance | `--json` | `effigy.help.v1` (inside command envelope) | `021-quick-start-and-command-cookbook.md` |
| `effigy tasks` | List discovered catalogs/tasks and probe routing | `--repo`, `--task`, `--resolve`, `--json`, `--pretty true\|false` | `effigy.tasks.v1`, `effigy.tasks.filtered.v1` | `016-task-routing-precedence.md` |
| `effigy doctor` | Run health checks and optional explain-mode selection diagnostics | `--repo`, `--fix`, `--verbose`, `--json` | `effigy.doctor.v1`, `effigy.doctor.explain.v1` | `018-doctor-explain-mode.md` |
| `effigy test` | Run built-in or explicit `tasks.test` test orchestration | `--plan`, `--verbose-results`, `--tui`, `--json` | `effigy.test.plan.v1`, `effigy.test.results.v1` | `013-testing-orchestration.md` |
| `effigy watch` | Policy-first file-triggered reruns for a target task | `--owner`, `--debounce-ms`, `--include`, `--exclude`, `--once`, `--max-runs`, `--json` | `effigy.watch.v1` (bounded JSON runs) | `019-watch-init-migrate-phase-1.md` |
| `effigy init` | Scaffold baseline `effigy.toml` | `--dry-run`, `--force`, `--json` | `effigy.init.v1` | `019-watch-init-migrate-phase-1.md` |
| `effigy migrate` | Import `package.json` scripts into `[tasks]` | `--from`, `--script`, `--apply`, `--json` | `effigy.migrate.v1` | `019-watch-init-migrate-phase-1.md` |
| `effigy config` | Render config reference or schema snippets | `--schema`, `--minimal`, `--target`, `--runner`, `--json` | `effigy.config.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy unlock` | Clear lock scopes manually | `--all`, `--json` | `effigy.unlock.v1` | `020-dag-lock-policy-baseline.md` |
| `effigy <task>` / `effigy <catalog>/<task>` | Run manifest-defined tasks with routing rules | passthrough args, `--json` | `effigy.task.run.v1` | `022-manifest-cookbook.md` |

## 2) Global JSON Envelope

For sample payloads per schema, see [`026-json-payload-examples.md`](./026-json-payload-examples.md).


Canonical machine mode:

```sh
effigy --json <command>
```

All command JSON responses are wrapped in:
- envelope schema: `effigy.command.v1`
- command-specific payload in `result` (or `error.details` for some failures)

See [`017-json-output-contracts.md`](./017-json-output-contracts.md) for envelope and payload details.

## 3) Command Shapes

```sh
effigy tasks [--repo <PATH>] [--task <TASK_NAME>] [--resolve <SELECTOR>] [--json] [--pretty true|false]
effigy doctor [--repo <PATH>] [--fix] [--verbose] [--json]
effigy doctor [--repo <PATH>] <task> -- <args> [--json]
effigy test [--plan] [--verbose-results] [--tui] [suite] [runner args]
effigy watch --owner <effigy|external> [--debounce-ms <MS>] [--include <GLOB>] [--exclude <GLOB>] <task> [task args]
effigy watch --owner effigy --once <task> [task args]
effigy init [--dry-run] [--force] [--json]
effigy migrate [--from <PATH>] [--script <NAME>]... [--apply] [--json]
effigy config [--schema] [--minimal] [--target <section>] [--runner <runner>] [--json]
effigy unlock [--all | <scope>...] [--json]
```

## 4) Scope Notes and Constraints

- `tasks --pretty false` is valid only with `--json`.
- `watch --json` requires bounded mode (`--once` or `--max-runs`).
- `watch --owner` is required; `external` owner blocks nested watch loops.
- `config --minimal` requires `--schema`.
- `config --runner` requires `--schema --target test`.
- `unlock` accepts either explicit scopes or `--all` (not both).

## 5) Common Recipes

Routing diagnosis:

```sh
effigy tasks --resolve test
effigy doctor --repo /path/to/workspace app/build -- --watch
```

Test planning and execution:

```sh
effigy test --plan
effigy test vitest
```

CI/machine-mode:

```sh
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

Lock recovery:

```sh
effigy unlock task:watch:test
effigy unlock --all
```

## Related Guides

- [`017-json-output-contracts.md`](./017-json-output-contracts.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)
