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
- `workspace`
- `task:<name>`
- `profile:<task>/<profile>`

Recovery command:

```sh
effigy unlock task:watch:test
```

## Explain Mode

Definition:
- Doctor mode that reports selector candidates, selection reasoning, and deferral consideration.

Example:

```sh
effigy doctor api/build -- --watch
```

## Plan Mode

Definition:
- Non-executing preview mode for built-in test routing.

Example:

```sh
effigy test --plan
```

## Managed TUI Task

Definition:
- Task with `mode = "tui"` and `concurrent` process entries, rendered in multiprocess UI.

Example:

```toml
[tasks.dev]
mode = "tui"
concurrent = [{ run = "cargo run -p api", start = 1, tab = 1 }]
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
