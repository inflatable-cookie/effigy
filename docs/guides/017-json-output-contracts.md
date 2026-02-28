# JSON Output Contracts

Effigy supports root-level JSON mode via `--json`:

```bash
effigy --json tasks
effigy --json tasks --task test
effigy --json tasks --resolve farmyard/api
effigy --json tasks --task test --resolve farmyard/test
effigy --json repo-pulse
effigy --json test --plan
effigy --json test
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
  "managed_profiles": [],
  "builtin_tasks": [],
  "catalogs": [],
  "precedence": [],
  "resolve": null
}
```

`--resolve` attaches routing probe details to the same `effigy.tasks.v1` payload:

```bash
effigy --json tasks --resolve farmyard/api
effigy --json tasks --resolve test
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
  "managed_profile_matches": [],
  "builtin_matches": [],
  "catalogs": [],
  "precedence": [],
  "resolve": null,
  "notes": []
}
```

Managed profile rows in `managed_profiles` and `managed_profile_matches` use direct invocation labels in `task` (for example `dev front`, `dev admin`).

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

Machine-readable checker selection payload contract:

`docs/contracts/json-selection-contract.json`

Local artifact validator:

`scripts/validate-json-contract-selection-artifact.sh`

Validation script:

`scripts/check-json-contracts.sh`

Run locally or in CI:

```bash
./scripts/check-json-contracts.sh
```

Fast mode (skip heavy execution surfaces such as `effigy --json test`):

```bash
./scripts/check-json-contracts.sh --fast
```

Changed-only mode (validate only active schema entries changed since a git base ref):

```bash
./scripts/check-json-contracts.sh --changed-only origin/main
```

You can combine selectors:

```bash
./scripts/check-json-contracts.sh --fast --changed-only origin/main
```

Selection debug output (prints schema ids selected for validation):

```bash
./scripts/check-json-contracts.sh --fast --changed-only origin/main --print-selected
```

Machine-readable selection output:

```bash
./scripts/check-json-contracts.sh --fast --changed-only origin/main --print-selected=json
```

Artifact replay validation:

```bash
./scripts/validate-json-contract-selection-artifact.sh ./json-contracts-selected.json
```

CI policy:

- pull requests run `./scripts/check-json-contracts-ci.sh`:
  - first attempt: `./scripts/check-json-contracts.sh --fast --changed-only <pr-base-commit>`
  - fallback: `./scripts/check-json-contracts.sh --fast` when base ref cannot be resolved
- `main` pushes run `./scripts/check-json-contracts.sh`
- nightly scheduled runs execute `./scripts/check-json-contracts.sh`
- CI helper enables `--print-selected` so selected schema ids are visible in logs
- CI uploads `json-contracts.log` and `json-contracts-selected.json` artifacts for each run
- CI validates `json-contracts-selected.json` with `scripts/validate-json-contract-selection-artifact.sh` before artifact upload
- CI runs `scripts/check-selection-artifact-validator-smoke.sh` to verify validator failure-path behavior stays intact

Built-in selector probe example:

```bash
effigy --json tasks --resolve test
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
