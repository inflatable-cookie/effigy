# JSON Output Contracts

Effigy has one JSON mode:
- `--json`: canonical command envelope (`effigy.command.v1`) for CI/tooling.

```bash
effigy --json help
effigy --json tasks
effigy --json doctor
effigy --json test --plan
effigy --json <catalog-or-root-task>
```

When JSON mode is active, the CLI preamble is suppressed and output is pure JSON.

## Top-Level Contract

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": true,
  "command": {
    "kind": "task",
    "name": "build"
  },
  "result": {},
  "error": null
}
```

Failure envelope shape:

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": false,
  "command": {
    "kind": "task",
    "name": "missing-task"
  },
  "result": null,
  "error": {
    "kind": "RunnerError",
    "message": "...",
    "details": {}
  }
}
```

## Result Payload Schemas

`result` (or `error.details` for some failures) contains command-specific schemas:

- `effigy.help.v1`
- `effigy.tasks.v1`
- `effigy.tasks.filtered.v1`
- `effigy.doctor.v1`
- `effigy.doctor.explain.v1`
- `effigy.config.v1`
- `effigy.task.run.v1`
- `effigy.test.plan.v1`
- `effigy.test.results.v1`

Examples:

```bash
effigy --json tasks
effigy --json tasks --task test
effigy --json tasks --resolve catalog-a/api
effigy --json doctor
effigy --json doctor farmyard/build -- --watch
effigy --json config
effigy --json build --repo /path/to/workspace
effigy --json test --plan
effigy --json test
```

## Contract Validation

JSON contract smoke checks:

```bash
./scripts/check-json-contracts.sh --fast
./scripts/check-json-contracts.sh
```

Changed-only mode:

```bash
./scripts/check-json-contracts.sh --fast --changed-only-base origin/main
```

## Compatibility Notes

- `schema_version` is the top-level envelope version.
- New optional fields may be added in `v1` without removing existing keys.
- Breaking envelope changes require a new top-level schema/version.
