# 019 - Watch, Init, and Migrate (Phase 1)

This guide covers the phase-1 contract for:
- `effigy watch`
- `effigy init`
- `effigy migrate`

## `effigy watch`

Phase-1 watch mode is policy-first:
- owner policy is mandatory (`--owner <effigy|external>`)
- `external` owner fails fast to avoid nested watcher loops
- `effigy` owner enables file-triggered reruns with debounce and glob controls

### Usage

```sh
effigy watch --owner effigy --once test
effigy watch --owner effigy --debounce-ms 500 --include "src/**" --exclude "**/*.snap" test vitest user-service
effigy watch --owner external test
```

### Notes

- `--json` is supported for bounded runs only (`--once` or `--max-runs <N>`).
- Default excludes include `.git/**`, `node_modules/**`, and `target/**`.
- Effigy acquires a watch-owner lock scope per target (`task:watch:<target>`); concurrent
  owners for the same target fail fast with lock diagnostics.
- If a watch lock must be cleared manually: `effigy unlock task:watch:<target>`.

## `effigy init`

`init` creates a baseline `effigy.toml` scaffold with:
- a minimal valid `[tasks]` section
- commented managed-task example (`mode = "tui"`)
- commented DAG-style run sequence example

### Usage

```sh
effigy init
effigy init --dry-run
effigy init --force
effigy init --json
```

### Safety

- If `effigy.toml` already exists, `init` fails unless `--force` is set.
- `--dry-run` never writes files.

## `effigy migrate`

`migrate` imports `package.json` scripts into `[tasks]` with preview-first behavior.

### Usage

```sh
effigy migrate
effigy migrate --script build --script test
effigy migrate --apply
effigy migrate --from ./frontend/package.json --apply --json
```

### Behavior

- Source is `package.json` by default (`--from` overrides).
- Preview mode does not write files.
- `--apply` writes only ready imports.
- Existing task-name conflicts are skipped and reported with manual remediation guidance.
- `package.json` is never modified by migration.

## JSON Schemas

- `effigy.watch.v1` for bounded watch runs (`--json` + bounded mode)
- `effigy.init.v1` for init reports
- `effigy.migrate.v1` for migration previews/applies
