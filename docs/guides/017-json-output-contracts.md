# JSON Output Contracts

Effigy supports root-level JSON mode via `--json`:

```bash
effigy --json tasks
effigy --json tasks --task test
effigy --json repo-pulse
effigy --json catalogs --resolve farmyard/api
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

## Compatibility policy

- `schema` and `schema_version` are required for machine consumers.
- New optional fields may be added in `v1` without breaking existing keys.
- Renames/removals or type changes require a new schema id/version (for example `v2`).
