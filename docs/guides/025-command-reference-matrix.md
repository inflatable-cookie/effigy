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
| `effigy cache` | Inspect and invalidate phase-1 cache metadata | `inspect`, `invalidate`, `--all`, `--json` | `effigy.cache.v1` | `022-manifest-cookbook.md` |
| `effigy completion` | Generate shell completion scripts and selector candidates | `bash\|zsh\|fish`, `candidates`, `--repo`, `--prefix`, `--json` | `effigy.completion.v1`, `effigy.completion.candidates.v1` | `021-quick-start-and-command-cookbook.md` |
| `effigy <task>` / `effigy <catalog>/<task>` | Run manifest-defined tasks with routing rules | passthrough args, `--json` | `effigy.task.run.v1` | `022-manifest-cookbook.md` |

## 2) Global JSON Envelope

For sample payloads per schema, see [`026-json-payload-examples.md`](./026-json-payload-examples.md).


Canonical JSON mode:

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
effigy cache inspect [<selector>] [--json]
effigy cache invalidate [<selector>...] [--all] [--json]
effigy completion <bash|zsh|fish> [--json]
effigy completion candidates [--repo <PATH>] [--prefix <value>] [--json]
```

## 4) Scope Notes and Constraints

- `tasks --pretty false` is valid only with `--json`.
- `watch --json` requires bounded mode (`--once` or `--max-runs`).
- `watch --owner` is required; `external` owner blocks nested watch loops.
- `config --minimal` requires `--schema`.
- `config --runner` requires `--schema --target test`.
- `unlock` accepts either explicit scopes or `--all` (not both).
- `cache` phase-1 works only for tasks with explicit `[tasks.<name>.cache]` opt-in.
- `cache invalidate` accepts selectors or `--all` (not both).
- task-local runtime env is supported with `env = { KEY = "value" }` under task definitions (full table or compact inline table).
- run arrays also support env directives (`{ env = { ... } }` or `{ env = "<profile>" }`) to update env for later entries in the chain.
- run-array env directives also support cross-catalog indirection via `env = "<catalog-path>/<name>"` (path relative to current catalog unless absolute).
- top-level `[env]` defines reusable named entries for run-array indirection; entries can be direct values (`NAME = "value"`) or grouped profile arrays (`name = [{ KEY = "value" }, ...]`).
- when a named env entry is not defined in `[env]`, effigy falls back to process env and `.env` lookup for that key.
- task-level `env_file` and run-step `{ env_file = ... }` can override dotenv fallback source (`.env` by default); supports string or ordered array where first file containing a key wins.
- run-step `env`/`env_file` directives can be standalone no-op state updates (no `run` or `task` key required).
- for cross-catalog `env = "<catalog-path>/<name>"`, dotenv fallback uses that target catalog root (including `env_file` overrides) and does not check process env.
- `tasks.<name>.env` values support `{project}` and `{repo}` catalog-root token substitution.
- built-in `test` automatically applies manifest `[env]` `CARGO_*` values to cargo suites (`cargo-nextest` and `cargo-test`), including grouped profile entries.
- built-in cargo-env auto-apply matching accepts optional `env`/`exec`/`command` wrappers, leading `KEY=value` assignments, and path-qualified cargo binaries.
- built-in cargo-env auto-apply intentionally does not match shell-wrapped commands such as `sh -lc "cargo test --workspace"`.
- `completion` command list is sourced from the built-in command index (`BUILTIN_TASKS`) to reduce drift with command discovery output.
- `completion candidates` includes built-ins plus discovered `<task>` and `<catalog>/<task>` selectors.
- `completion candidates` JSON payload reports `cache_hit`, `cache_state`, `cache_age_ms` (on hit), `cache_ttl_ms` (on hit), `effective_cache_ttl_ms`, `cache_ttl_source`, and `manifest_count` for memoized candidate scans.
- `cache_state` values: `miss_initial`, `hit`, `miss_ttl`, `miss_manifest_change`.
- `miss_manifest_change` is triggered from manifest stamp drift (mtime/size/content digest), so cache invalidation is not dependent on timestamp granularity alone.
- Completion candidates cache TTL can be tuned with `EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS` (bounded to `100..60000`, default `2000`).
- `cache_ttl_source` values: `default`, `env`, `env_invalid` (invalid env values fall back to default TTL).

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

CI/JSON mode:

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
- [`034-task-and-command-glossary.md`](./034-task-and-command-glossary.md)
