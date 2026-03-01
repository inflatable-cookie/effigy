# JSON Output Contracts

Effigy has one canonical JSON mode:
- `--json`: command envelope (`effigy.command.v1`) for CI/tooling.

```bash
effigy --json help
effigy --json tasks
effigy --json doctor
effigy --json test --plan
effigy --json watch --owner effigy --once test
effigy --json <catalog-or-root-task>
```

When JSON mode is active, CLI preamble output is suppressed and output is pure JSON.

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

`result` (or `error.details` for some failures) contains command-specific schemas.

Current command payload schemas include:

Payload examples guide: `026-json-payload-examples.md`

- `effigy.help.v1`
- `effigy.tasks.v1`
- `effigy.tasks.filtered.v1`
- `effigy.doctor.v1`
- `effigy.doctor.explain.v1`
- `effigy.config.v1`
- `effigy.test.plan.v1`
- `effigy.test.results.v1`
- `effigy.watch.v1`
- `effigy.init.v1`
- `effigy.migrate.v1`
- `effigy.unlock.v1`
- `effigy.task.run.v1`

Examples:

```bash
effigy --json tasks
effigy --json tasks --task test
effigy --json tasks --resolve catalog-a/api
effigy --json doctor
effigy --json doctor --repo /path/to/workspace catalog-a/build -- --watch
effigy --json config
effigy --json config --schema --target test
effigy --json test --plan
effigy --json test
effigy --json watch --owner effigy --once test
effigy --json init --dry-run
effigy --json migrate --apply
effigy --json unlock --all
effigy --json build --repo /path/to/workspace
```

## Payload Examples

See `026-json-payload-examples.md` for realistic sample responses for each schema.

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

## Layered Contract Strategy

Effigy validates JSON in two layers:

| Layer | Scope | Primary tests |
|---|---|---|
| Runner payload contracts | Command-specific payload schema/shape (for example `effigy.watch.v1`, `effigy.init.v1`, `effigy.task.run.v1`) | `src/tests/json_contract_tests.rs` |
| CLI envelope contracts | Top-level `effigy.command.v1` envelope, `command.kind/name`, and error wrapping/remediation propagation | `tests/cli_output_tests.rs` |

Rule of thumb:
- Add payload/schema assertions in `json_contract_tests`.
- Add envelope/wrapping assertions in `cli_output_tests`.
- Keep behavior/runtime semantics in `src/tests/runner_tests.rs`.

## Compatibility Notes

- `schema_version` is the top-level envelope version.
- New optional fields may be added in `v1` without removing existing keys.
- Breaking envelope changes require a new top-level schema/version.

## Next Reading

- Watch/init/migrate command contracts: [`019-watch-init-migrate-phase-1.md`](./019-watch-init-migrate-phase-1.md)
- DAG/policy/locking behavior: [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
- CI automation patterns: [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)
