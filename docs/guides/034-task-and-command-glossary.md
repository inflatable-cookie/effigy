# 034 - Task and Command Glossary

Canonical term definitions for Effigy docs, commands, and JSON contracts.

## Catalog

Definition:
- A discovered `effigy.toml` unit that owns tasks, identified by `[catalog].alias`.

Example:

```toml
[catalog]
alias = "api"
```

## Task runtime prefix flags

Definition:
- Optional leading arguments on `effigy <task>` / `effigy <catalog>/<task>`
  parsed before task-specific passthrough: `--repo <PATH>`,
  `--verbose-root`, `--env-schema <PATH>` (see
  [`050-env-schema-integration.md`](./050-env-schema-integration.md)).

Notes:
- They must stay in the prefix segment consumed by the runtime parser, not
  mixed arbitrarily into passthrough.
- Some built-ins that use the passthrough parser (`doctor`, `watch`, `scan`)
  reject `--verbose-root` and `--env-schema` on that built-in invocation.

## Selector

Definition:
- A selector string passed to Effigy.
- Forms: unprefixed (`test`) or prefixed (`api/test`).

Examples:

```sh
effigy test
effigy api/test
```

## Routing

Definition:
- The process Effigy uses to resolve a selector to a catalog and task.

Inspection command:

```sh
effigy tasks --resolve test
```

## Deferral

Definition:
- Fallback execution path used when no selector matches local task definitions.

Example:

```toml
[defer]
run = "composer global exec effigy -- {request} {args}"
builtins = ["release"]
```

## Command Envelope

Definition:
- Top-level machine-readable wrapper used in JSON mode.

Canonical schema:
- `effigy.command.v1`

Example:

```sh
effigy --json doctor
```

## Payload Schema

Definition:
- Command-specific JSON contract inside envelope `result` (or `error.details`).

Examples:
- `effigy.tasks.v1`
- `effigy.doctor.v1`
- `effigy.test.plan.v1`
- `effigy.graph.explore.v1`
- `effigy.graph.affected.v1`

## Suite

Definition:
- A test runner target selected by built-in test orchestration.

Examples:
- `vitest`
- `cargo-nextest`
- `cargo-test`

Example command:

```sh
effigy test vitest
```

## Profile

Definition:
- Named managed-task variant under `[tasks.<name>.profiles.<profile>]`.

Example:

```toml
[tasks.dev.profiles.admin]
concurrent = [{ run = "bun run admin:dev", start = 1, tab = 1 }]
```

## Task Environment

Definition:
- Per-task runtime environment map defined as `tasks.<name>.env`.
- Keys are environment variable names, values are strings injected for that task's command execution.
- Run arrays also support `env` directive steps (`{ env = { ... } }` or `{ env = "<profile>" }`) that update env for later entries in that chain.
- `env = "<catalog-path>/<name>"` in run arrays resolves named env entries from another catalog's `[env]` table (relative to current catalog unless absolute).
- Named entries are declared under top-level `[env]` as either direct values (`NAME = "value"`) or grouped profile arrays (`name = [{ KEY = "value" }, ...]`).
- If a named entry is missing in `[env]`, effigy falls back to process env and `<catalog-root>/.env` for that key.
- Cross-catalog named refs (`env = "<catalog-path>/<name>"`) use target-catalog `[env]` and dotenv only (no process-env fallback).
- `tasks.<name>.env_file` (or run-step `env_file` directives) can point fallback resolution at one file or an ordered file list.
- `env` and `env_file` directive entries may be standalone no-op steps used to mutate env state before later `run`/`task` entries.
- Dotenv parsing accepts `KEY=value` or `export KEY=value` lines; matching single/double quotes are stripped from values.

Example:

```toml
[tasks.build]
run = "cargo build --workspace"
env = { CARGO_HOME = "{project}/.effigy/cargo/home", CARGO_TARGET_DIR = "{project}/.effigy/cargo/target" }
```

## Task Template Tokens

Definition:
- Placeholders available in task-related templates.
- `run` command templates support `{project}`, `{repo}`, and `{args}`.
- `tasks.<name>.env` values support `{project}` and `{repo}`.

Notes:
- `{project}` and `{repo}` both resolve to the selected catalog root path.

## Lock Scope

Definition:
- Runtime lock key used to avoid conflicting executions.

Common scopes:
- `task:<name>` by default
- `shared:<name>` for explicit cross-task serialization
- `profile:<task>/<profile>`

Recovery command:

```sh
effigy tasks unlock shared:dev-stack
effigy tasks unlock task:watch:test
```

## Explain Mode

Definition:
- Doctor mode that reports selector candidates, selection reasoning, and deferral consideration.

Example:

```sh
effigy doctor api/build --watch
```

## Plan Mode

Definition:
- Non-executing preview mode for built-in test routing.

Example:

```sh
effigy test --plan
```

## Code Graph

Definition:
- Effigy's local deterministic repo index under `.effigy/graph/graph.db`, built
  by first-party extractors and queried through `effigy repo graph`.

Notes:
- it is a navigation aid for agents and humans, not compiler-grade semantic truth
- queries do not rebuild the index; run `effigy repo graph index` explicitly

Deep dive:
- [`076-code-graph-and-agent-workflows.md`](./076-code-graph-and-agent-workflows.md)

## Graph Explore

Definition:
- One-call agent navigation command that returns primary owners, excerpts,
  relations, freshness, overflow, and guidance under `effigy.graph.explore.v1`.

Example:

```sh
effigy repo graph explore "trace release orchestrator" --max-files 6 --max-bytes 12288 --json
```

## Graph Affected

Definition:
- Changed-file validation narrowing command that turns edited paths into
  affected files, likely test files, and candidate Effigy test tasks under
  `effigy.graph.affected.v1`.

Example:

```sh
git diff --name-only | effigy repo graph affected --stdin --json
```

## Graph Freshness State

Definition:
- Compact trust label on graph query payloads under `freshness.state`, paired with
  `freshness.usable` and `freshness.summary`.

Values:

- `ready` — safe to trust navigation queries
- `refresh-recommended` — reindex before trusting queries
- `degraded` — partial problems; treat output as bounded guidance
- `missing-index` — run `graph index` first

## Graph Watch Event

Definition:
- Newline-delimited JSON event emitted by `effigy repo graph watch --json` with schema
  `effigy.graph.watch.event.v1`.

Notes:
- this is a streaming exception and does not use the one-shot `effigy.command.v1`
  envelope

## Managed TUI Task

Definition:
- Task with `mode = "tui"` and `concurrent` process entries, rendered in multiprocess UI.
- Individual `concurrent` entries can set `shutdown_on_exit = true` when one
  process should end the whole managed session on exit.

Example:

```toml
[tasks.dev]
mode = "tui"
concurrent = [{ run = "cargo run -p api", start = 1, tab = 1, shutdown_on_exit = true }]
```

## Notes

- Use these exact terms in docs where possible.
- Prefer linking this glossary rather than redefining terms repeatedly.

## Expected Outcome

- docs use one canonical meaning for `catalog`, `selector`, `routing`, `deferral`, `suite`, and `profile`
- cross-guide wording drift is reduced during future updates

## Related Guides

- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`033-style-and-terminology-guide.md`](./033-style-and-terminology-guide.md)
- [`017-json-output-contracts.md`](./017-json-output-contracts.md)

## Next Step

When introducing new command surfaces or schema terms, add definitions here first and then propagate usage across affected guides.
