# JSON Output Contracts

Effigy supports root-level JSON mode via `--json`:

```bash
effigy --json tasks
effigy --json tasks --task test
effigy --json repo-pulse
effigy --json catalogs --resolve farmyard/api
effigy --json test --plan
effigy --json test
effigy --json catalogs --resolve test
```

When JSON mode is active, the CLI preamble is suppressed and output is pure JSON.

## Tasks (unfiltered)

Command:

```bash
effigy --json tasks
```

Contract:

```json
{
  "schema": "effigy.tasks.v1",
  "schema_version": 1,
  "catalog_count": 0,
  "catalog_tasks": [],
  "builtin_tasks": []
}
```

## Tasks (filtered)

Command:

```bash
effigy --json tasks --task test
```

Contract:

```json
{
  "schema": "effigy.tasks.filtered.v1",
  "schema_version": 1,
  "catalog_count": 0,
  "filter": "test",
  "matches": [],
  "builtin_matches": [],
  "notes": []
}
```

## Repo Pulse

Command:

```bash
effigy --json repo-pulse
```

Contract:

```json
{
  "schema": "effigy.repo-pulse.v1",
  "schema_version": 1,
  "report": {
    "repo": "/abs/path",
    "owner": "platform",
    "eta": "phase-22",
    "evidence": [],
    "risk": [],
    "next_action": []
  },
  "root_resolution": {
    "resolved_root": "/abs/path",
    "mode": "AutoNearest",
    "evidence": [],
    "warnings": []
  }
}
```

## Catalogs

Command:

```bash
effigy --json catalogs --resolve farmyard/api
```

Contract:

```json
{
  "schema": "effigy.catalogs.v1",
  "schema_version": 1,
  "catalogs": [],
  "precedence": [],
  "resolve": null
}
```

## Built-in Test Plan

Command:

```bash
effigy --json test --plan
```

Contract:

```json
{
  "schema": "effigy.test.plan.v1",
  "schema_version": 1,
  "request": "test",
  "root": "/abs/path",
  "runtime": "text",
  "targets": [],
  "recovery": null
}
```

## Built-in Test Results

Command:

```bash
effigy --json test
```

Contract:

```json
{
  "schema": "effigy.test.results.v1",
  "schema_version": 1,
  "targets": [],
  "failures": [],
  "hint": null
}
```

## Schema Index

Machine-readable schema map:

`docs/contracts/json-schema-index.json`

Built-in selector probe example:

```bash
effigy --json catalogs --resolve test
```

Expected `resolve` shape:

```json
{
  "selector": "test",
  "status": "ok",
  "catalog": null,
  "catalog_root": null,
  "task": "test",
  "evidence": ["resolved built-in task `test`"],
  "error": null
}
```

## Compatibility policy

- `schema` and `schema_version` are required for machine consumers.
- New optional fields may be added in `v1` without breaking existing keys.
- Renames/removals or type changes require a new schema id/version (for example `v2`).
