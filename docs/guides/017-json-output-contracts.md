# JSON Output Contracts

Use this guide when Effigy output needs to be consumed by CI, agents, or other
tools instead of a human reading terminal text.

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


## Vision Alignment

- Primary tags: `CONTRACT`, `RELEASE`
- Target movement: JSON envelopes stay canonical so CI/tooling integrations remain stable across command growth.

## Start Here

If you are automating Effigy for the first time, use this mental model:

- `effigy --json <command>` is the only canonical machine-facing path
- every JSON response is wrapped in `effigy.command.v1`
- the command-specific payload lives in `result` or, for some failures,
  `error.details`

Start with:

```bash
effigy --json tasks
effigy --json doctor
effigy --json test --plan
```

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
- `effigy.scan.god-files.v1`
- `effigy.scan.duplicate-blocks.v1`
- `effigy.scan.comment-ratio.v1`
- `effigy.scan.generated-assets.v1`
- `effigy.scan.generated-in-src.v1`
- `effigy.scan.attention-markers.v1`
- `effigy.scan.stale-suppressions.v1`
- `effigy.config.v1`
- `effigy.test.plan.v1`
- `effigy.test.results.v1`
- `effigy.watch.v1`
- `deploy.model.v1`
- `effigy.deploy.export.v1`
- `effigy.init.v1`
- `effigy.init.list.v1`
- `effigy.migrate.v1`
- `effigy.unlock.v1`
- `effigy.completion.v1`
- `effigy.completion.candidates.v1`
- `effigy.task.run.v1`

Examples:

```bash
effigy --json tasks
effigy --json tasks --task test
effigy --json tasks --resolve catalog-a/api
effigy --json doctor
effigy --json doctor --repo /path/to/workspace catalog-a/build -- --watch
effigy --json scan god-files
effigy --json scan duplicate-blocks
effigy --json scan comment-ratio
effigy --json scan generated-assets
effigy --json scan generated-in-src
effigy --json scan attention-markers
effigy --json scan stale-suppressions
effigy --json config
effigy --json config --schema --target test
effigy --json deploy model --repo /path/to/workspace
effigy --json deploy export render --repo /path/to/workspace --path infra/render --plan
effigy --json deploy export railway --repo /path/to/workspace --path infra/railway --plan
effigy --json test --plan
effigy --json test
effigy --json watch --owner effigy --once test
effigy --json init --dry-run
effigy --json migrate --apply
effigy --json unlock --all
effigy --json completion bash
effigy --json completion candidates --prefix farm
effigy --json build --repo /path/to/workspace
```

## Payload Examples

See `026-json-payload-examples.md` for realistic sample responses for each schema.

### Completion Candidates Telemetry (`effigy.completion.candidates.v1`)

`effigy --json completion candidates` includes cache diagnostics for selector memoization:

- `cache_hit` (boolean): whether candidates were served from in-process cache.
- `cache_state` (string): `miss_initial`, `hit`, `miss_ttl`, or `miss_manifest_change`.
- `cache_age_ms` (number|null): cache entry age on hit; `null` on miss.
- `cache_ttl_ms` (number|null): hit-scoped TTL value; `null` on miss.
- `effective_cache_ttl_ms` (number): active TTL policy used for this response.
- `cache_ttl_source` (string): TTL source policy:
  - `default` when no env override is set
  - `env` when `EFFIGY_COMPLETION_CANDIDATES_CACHE_TTL_MS` is valid
  - `env_invalid` when that env var is set but malformed (falls back to default TTL)
- `manifest_count` (number): number of manifest sources included in candidate discovery.

### Doctor vs Scan Payloads

- `effigy --json doctor` is the integrated health report. Scanner-backed findings like `scan.god-files` are normalized into doctor `sections` and flattened `findings`. Plain-text `effigy doctor` summarizes those sections and writes file-level scan detail reports under `.effigy/reports/doctor/`.
- `effigy --json scan god-files` is the raw scanner payload. Use it when you need the full findings list, scan-local text snapshot, or report-output metadata.
- `effigy --json scan duplicate-blocks` is the raw duplication payload. Use it when you need normalized block spans, occurrence locations, and snippet fingerprints without doctor normalization.
- `effigy --json scan comment-ratio` is the raw comment-heaviness payload. Use it when you need per-file comment/code counts and ratio classifications without doctor normalization.
- `effigy --json scan generated-assets` is the raw bulky-artifact payload. Use it when you need the vendored/generated asset list without doctor normalization.
- `effigy --json scan generated-in-src` is the raw source-tree boundary payload. Use it when you need generated-file findings scoped to maintained source paths without doctor normalization.
- `effigy --json scan attention-markers` is the raw attention-marker payload. Use it when you need the full marker list, line numbers, and text snapshot without doctor normalization.
- `effigy --json scan stale-suppressions` is the raw suppression-marker payload. Use it when you need the full list of inline lint/type/tool bypasses without doctor normalization.

## Contract Validation

JSON contract smoke checks:

```bash
effigy contracts check-json --fast
effigy contracts check-json --full
```

Changed-only mode:

```bash
effigy contracts check-json --fast --changed-only origin/main
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

## Expected Outcome

After this guide, you should be able to:

- identify the stable envelope Effigy uses for machine consumers
- know where command-specific payload data lives inside the envelope
- choose the right validation path when a JSON contract changes

## Related Guides

- Watch/init/migrate command contracts: [`019-watch-init-migrate-foundation.md`](./019-watch-init-migrate-foundation.md)
- DAG/policy/locking behavior: [`020-dag-lock-policy-baseline.md`](./020-dag-lock-policy-baseline.md)
- CI automation patterns: [`024-ci-and-automation-recipes.md`](./024-ci-and-automation-recipes.md)

## Next Step

After updating any envelope or payload shape, add or refresh examples in [`026-json-payload-examples.md`](./026-json-payload-examples.md) and run `effigy contracts check-json --fast`.
