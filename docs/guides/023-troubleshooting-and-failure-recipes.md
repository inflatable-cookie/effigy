# 023 - Troubleshooting and Failure Recipes

This guide maps common Effigy failures to fast diagnosis commands and concrete fixes.

## 1) Fast Triage Flow

Run these first:

```sh
effigy --help
effigy tasks
effigy tasks --resolve test
effigy doctor --verbose
```

If automation is involved, capture JSON:

```sh
effigy --json tasks --resolve test
effigy --json doctor
```

## 2) Task Resolution Failures

### Symptom: `task catalog prefix <...> not found`

Diagnosis:

```sh
effigy tasks
effigy tasks --json
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
pwd
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

## 4) Built-in Test Routing Errors

### Symptom: `built-in test is ambiguous for arguments ...`

Diagnosis:

```sh
effigy test --plan <args>
```

Fix:
- choose suite explicitly:

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
effigy --json test
```

Fix:
- inspect per-target command/exit diagnostics,
- rerun failing suite directly in that catalog root.

## 5) Watch Mode Errors

### Symptom: ``--owner <effigy|external> is required``

Fix:

```sh
effigy watch --owner effigy --once test
```

### Symptom: `watch owner external means task-managed watching is expected`

Fix:
- do not wrap external watcher tasks with `effigy watch`,
- run the watcher task directly.

### Symptom: ``--json requires a bounded watch run``

Fix:

```sh
effigy watch --owner effigy --once test --json
# or
effigy watch --owner effigy --max-runs 2 --json test
```

### Symptom: lock conflict for `task:watch:<target>`

Diagnosis/Fix:

```sh
effigy unlock task:watch:<target>
# if needed
effigy unlock --all
```

## 6) Lock Conflicts and Stale Locks

### Symptom: `lock conflict for <scope> ...`

Diagnosis:
- read reported scope and lock path in the error output.

Fix:

```sh
effigy unlock workspace
effigy unlock task:<name>
effigy unlock profile:<task>/<profile>
effigy unlock --all
```

Use `--all` only when you cannot isolate a safe scope.

## 7) Managed TUI Task Failures

### Symptom: `managed task <...> profile <...> not found`

Diagnosis:

```sh
effigy tasks --task <task-name>
```

Fix:
- define `[tasks.<name>.profiles.<profile>]`, or
- run existing profile.

### Symptom: managed mode invalid / empty / bad task ref

Diagnosis:

```sh
effigy doctor --verbose
effigy <managed-task>
```

Fix:
- ensure `mode = "tui"` and `concurrent = [...]` exist,
- validate each `task = "<selector>"` reference resolves.

## 8) Config / Migrate / CLI Argument Errors

### Symptom: `unknown argument` or `requires a value`

Diagnosis:

```sh
effigy --help
effigy <command> --help
```

Fix:
- correct flag shape and placement,
- for machine usage, prefer `effigy --json <command>`.

### Symptom: `migration source not found`

Fix:

```sh
effigy migrate --from ./path/to/package.json
```

### Symptom: config target/runner validation errors

Valid examples:

```sh
effigy config --schema --target test
effigy config --schema --target test --runner vitest
```

## 9) Deferral Failures

### Symptom: `deferral loop detected (...)`

Diagnosis:
- the unresolved request is bouncing through defer rules repeatedly.

Fix:
- tighten `[defer].run` conditions in the active manifest,
- avoid recursive calls that re-invoke the same unresolved selector chain.

## 10) When to Use `doctor` vs `tasks --resolve`

Use `effigy tasks --resolve <selector>` when you need routing evidence only.

Use `effigy doctor <selector> <args>` when you need full explain output with selection and deferral reasoning.

## Related Guides

- [`016-task-routing-precedence.md`](./016-task-routing-precedence.md)
- [`018-doctor-explain-mode.md`](./018-doctor-explain-mode.md)
- [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md)
- [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
- [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
