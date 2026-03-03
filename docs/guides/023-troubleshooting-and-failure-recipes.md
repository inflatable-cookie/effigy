# 023 - Troubleshooting and Failure Recipes

This guide maps common Effigy failures to focused diagnosis steps and concrete fixes.

Use [`025-command-reference-matrix.md`](./025-command-reference-matrix.md) for full command syntax and flag details.

## 1) Fast Triage Flow

Start with:

```sh
effigy tasks --resolve test
effigy doctor --verbose
```

For CI/automation cases, capture machine output:

```sh
effigy --json tasks --resolve test
effigy --json doctor
```

## 2) Task Resolution Failures

### Symptom: `task catalog prefix <...> not found`

Diagnosis:

```sh
effigy tasks
```

Fix:
- use a real catalog alias from `effigy tasks`, or
- update `[catalog].alias` in the target `effigy.toml`, then retry.

### Symptom: `task <...> is not defined in discovered catalogs`

Diagnosis:

```sh
effigy tasks --task <task-name>
effigy tasks --resolve <task-name>
```

Fix:
- add the missing task under `[tasks]` in the right catalog, or
- call an existing prefixed task (`<catalog>/<task>`).

### Symptom: `task <...> is ambiguous; matched multiple catalogs`

Diagnosis:

```sh
effigy tasks --resolve <task-name>
```

Fix:
- run with explicit prefix (`<catalog>/<task>`), or
- run from a deeper directory to trigger nearest in-scope resolution.

## 3) Catalog Discovery and Manifest Issues

### Symptom: `no task catalogs found under ...`

Diagnosis:

```sh
find . -name effigy.toml
```

Fix:
- run from correct workspace root, or
- create a manifest with `effigy init`.

### Symptom: `duplicate task catalog alias <...> found in ...`

Diagnosis:

```sh
effigy tasks
```

Fix:
- make aliases unique across all discovered manifests.

### Symptom: `failed to parse ...effigy.toml`

Diagnosis:

```sh
effigy doctor --verbose
```

Fix:
- correct TOML syntax and key names,
- remove unsupported keys (manifest uses strict unknown-field rejection).

### Symptom: `task <...> run step references unknown env entry <...>`

Diagnosis:

```sh
effigy tasks --resolve <task-selector>
```

Inspect the selected catalog `effigy.toml`:
- verify `[env]` contains the referenced entry name (`env = "<name>"`), or
- verify cross-catalog path refs (`env = "../shared/<name>"`) point to a real catalog root.

Fix:
- define the missing entry in `[env]`, or
- export it in process environment, or
- add it to `.env` / `env_file` fallback for that catalog.

### Symptom: `failed to read env file <...>`

Diagnosis:

```sh
effigy doctor --verbose
ls -la <catalog-root>/<env-file>
```

Fix:
- correct the `env_file` path (task-level or run-step),
- ensure file permissions allow read access,
- use an ordered fallback list (`env_file = [".env.local", ".env.test"]`) when files are optional per environment.

## 4) Built-in Test Routing Errors

### Symptom: `built-in test is ambiguous for arguments ...`

Diagnosis:

```sh
effigy test --plan <args>
```

Fix:

```sh
effigy test vitest <args>
effigy test nextest <args>
```

### Symptom: `built-in test runner <...> is not available ... Did you mean ...`

Diagnosis:

```sh
effigy test --plan <args>
```

Fix:
- use suggested suite name from error output,
- optionally define `[test.suites]` for explicit suite source-of-truth.

### Symptom: one or more built-in test targets failed

Diagnosis:

```sh
effigy test --verbose-results
```

Fix:
- inspect per-target command/exit diagnostics,
- rerun failing suite directly in that catalog root.

### Symptom: `CARGO_HOME` / `CARGO_TARGET_DIR` is missing during `effigy test`

Diagnosis:

```sh
effigy tasks --resolve test
effigy test --plan
```

Inspect:
- whether routing selected explicit `tasks.test` (instead of built-in `test`)
- whether the planned command is cargo-shaped vs shell-wrapped
- whether the selected catalog defines `[env]` `CARGO_*` entries

Fix:
- if you want built-in auto-apply behavior, avoid overriding with explicit `tasks.test`
- define `CARGO_*` values in the selected catalog `[env]`
- use cargo command forms directly (for example `cargo test ...`, `env FOO=bar cargo test ...`) instead of `sh -lc "cargo ..."` wrappers

### Symptom: built-in cargo suite command did not receive manifest `CARGO_*` env

Diagnosis:
- inspect command shape in `[test.suites]` and `effigy test --plan`

Fix:
- use a supported cargo executable form such as:

```sh
cargo test --workspace
cargo-nextest run --workspace
env RUST_LOG=info cargo test --workspace
/usr/local/bin/cargo nextest run --workspace
```

- avoid shell-string wrappers where cargo is not the executable token (for example `sh -lc "cargo test --workspace"`)

## 5) Watch Mode and Lock Errors

### Symptom: ``--owner <effigy|external> is required``

Fix:

```sh
effigy watch --owner effigy --once test
```

### Symptom: ``--json requires a bounded watch run``

Fix:

```sh
effigy watch --owner effigy --once test --json
# or
effigy watch --owner effigy --max-runs 2 --json test
```

### Symptom: lock conflict for `task:watch:<target>`

Fix:

```sh
effigy unlock task:watch:<target>
# if needed
effigy unlock --all
```

### Symptom: `lock conflict for <scope> ...`

Fix:

```sh
effigy unlock workspace
effigy unlock task:<name>
effigy unlock profile:<task>/<profile>
```

Use `--all` only when you cannot isolate a safe scope.

## 6) Managed TUI and Argument Errors

### Symptom: `managed task <...> profile <...> not found`

Diagnosis:

```sh
effigy tasks --task <task-name>
```

Fix:
- define `[tasks.<name>.profiles.<profile>]`, or
- run an existing profile.

### Symptom: `unknown argument` or `requires a value`

Diagnosis:

```sh
effigy <command> --help
```

Fix:
- correct flag shape and placement,
- for machine usage, prefer `effigy --json <command>`.

## 7) Deferral Failures

### Symptom: `deferral loop detected (...)`

Diagnosis:
- the unresolved request is bouncing through defer rules repeatedly.

Fix:
- tighten `[defer].run` conditions in the active manifest,
- avoid recursive calls that re-invoke the same unresolved selector chain.

## 8) When to Use `doctor` vs `tasks --resolve`

- use `effigy tasks --resolve <selector>` for routing evidence only,
- use `effigy doctor <selector> -- <args>` for full explain output including selection and deferral reasoning.

## Related Guides

- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

## Next Step

When symptom-level fixes are done, codify the preventative CI checks in [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md).
