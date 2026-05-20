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
effigy --json graph status
effigy --json graph context "trace release orchestrator"
```

## Top-Level Contract

```json
{
  "schema": "effigy.command.v1",
  "schema_version": 1,
  "ok": true,
  "binary": {
    "name": "effigy",
    "version": "0.5.0",
    "active_version": "v0.5.0+local.abc123",
    "display_version": "v0.5.0+local.abc123"
  },
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
  "binary": {
    "name": "effigy",
    "version": "0.5.0",
    "active_version": "v0.5.0+local.abc123",
    "display_version": "v0.5.0+local.abc123"
  },
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
- `effigy.tasks-status.v1`
- `effigy.tasks-status-all.v1`
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
- `effigy.bundle.sync.v1`
- `effigy.test.plan.v1`
- `effigy.test.results.v1`
- `effigy.watch.v1`
- `effigy.graph.index.v1`
- `effigy.graph.status.v1`
- `effigy.graph.search.v1`
- `effigy.graph.files.v1`
- `effigy.graph.node.v1`
- `effigy.graph.callers.v1`
- `effigy.graph.callees.v1`
- `effigy.graph.impact.v1`
- `effigy.graph.context.v1`
- `effigy.graph.explore.v1`
- `effigy.graph.affected.v1`
- `deploy.model.v1`
- `effigy.deploy.export.v1`
- `effigy.deploy.plan.v1`
- `effigy.deploy.apply.v1`
- `effigy.deploy.status.v1`
- `effigy.deploy.history.v1`
- `effigy.init.v1`
- `effigy.init.list.v1`
- `effigy.init.checklist.v1`
- `effigy.init.actions.v1`
- `effigy.migrate.v1`
- `effigy.unlock.v1`
- `effigy.completion.v2`
- `effigy.completion.candidates.v1`
- `effigy.task.run.v1`
- `effigy.artifact.inspect.v1`
- `effigy.artifact.stage.v1`
- `effigy.artifact.capture.v1`
- `effigy.state-stack.lineage.v1`
- `effigy.state-stack.apply.v1`

Examples:

```bash
effigy --json tasks
effigy --json tasks --task test
effigy --json tasks --resolve catalog-a/api
effigy --json tasks status test
effigy --json tasks status --all
effigy --json doctor
effigy --json doctor --repo /path/to/workspace catalog-a/build --watch
effigy --json scan god-files
effigy --json scan duplicate-blocks
effigy --json scan comment-ratio
effigy --json scan generated-assets
effigy --json scan generated-in-src
effigy --json scan attention-markers
effigy --json scan stale-suppressions
effigy --json config path
effigy --json config get containers.backend
effigy --json config --schema --target test
effigy --json graph index
effigy --json graph status
effigy --json graph search release --limit 10
effigy --json graph explore "trace deploy provider export" --max-files 6 --max-bytes 12288
effigy --json graph context "trace deploy provider export" --max-files 8 --max-bytes 4096
effigy --json graph affected src/runner/graph_command.rs --depth 2
effigy --json deploy model --repo /path/to/workspace
effigy --json deploy export <PROVIDER> --repo /path/to/workspace --path infra/deploy --plan
effigy --json deploy plan uat --repo /path/to/workspace
effigy --json deploy apply uat --repo /path/to/workspace --yes
effigy --json test --plan
effigy --json test
effigy --json watch --owner effigy --once test
effigy --json init --dry-run
effigy --json tasks migrate --apply
effigy --json tasks unlock --all --yes
effigy --json config completion bash --export
effigy --json config completion candidates --prefix farm
effigy --json state plan ./state/acowtancy-uat.toml
effigy --json state plan uat --write-report
effigy --json state apply uat
effigy --json state capture uat new-content
effigy --json state capture uat --role uat-capture --source-env uat --key uat-capture-2026-05-08
effigy --json state history uat --kind capture --limit 5
effigy --json build --repo /path/to/workspace
```

## Graph Watch Streaming Exception

`effigy graph watch --json` is intentionally different from the normal command
envelope model.

It is a long-running streaming command, so it emits newline-delimited JSON
events with schema:

- `effigy.graph.watch.event.v1`

It does **not** wrap each event in `effigy.command.v1`.

Use it like this:

```bash
effigy graph watch --json
```

Consume one JSON object per line. Current event kinds:

- `started`
- `refresh`
- `dirty`
- `reconcile`
- `fatal`

## Graph Workflow Notes

- `graph status --json` is the freshness gate; read `payload.freshness.state`
  and `payload.freshness.usable` before trusting queries. Reindex when state is
  `missing-index`, `refresh-recommended`, or `degraded`, or when `usable` is
  false. Path lists (`stale_paths`, `failed_paths`) are supporting detail.
- `graph affected --json` narrows validation scope but does not claim exhaustive
  test reachability.
- `graph explore --json` and `graph context --json` are bounded packets; exact
  token confirmation still belongs to `rg`.

## Payload Examples

See `026-json-payload-examples.md` for realistic sample responses for each schema.

### Completion Candidates Telemetry (`effigy.completion.candidates.v1`)

`effigy --json config completion candidates` includes cache diagnostics for selector memoization:

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

## Contracts Validation Workflow

Use `effigy contracts` when you need to validate JSON contract artifacts or check
that Effigy's own command payloads still conform to declared schemas.

### Fast vs full checks

```bash
# Quick check: validates only schemas that have fast validators
effigy contracts check-json --fast

# Complete check: validates every declared schema
effigy contracts check-json --full

# Check only schemas touched since a git ref (great for PRs)
effigy contracts check-json --fast --changed-only origin/main
```

Use `--fast` for daily local checks. Use `--full` before releases or when you
suspect a broad schema change. Use `--changed-only` in CI to keep PR checks fast.

### Validate a selection artifact

When CI produces a selection payload, gate it before using it:

```bash
# Generate a selection artifact
effigy contracts check-json --full --print-selected=json > contracts-selected.json

# Validate it independently
effigy contracts validate-selection --artifact ./contracts-selected.json
```

This checks:
- required keys exist,
- `count == length(selected)`,
- `selected` is string-only,
- `mode` is allowed (`fast` or `full`).

Use this in CI pipelines that pass contract selection data between jobs.

### Print selected schemas

```bash
# Human-readable list
effigy contracts check-json --fast --print-selected

# Machine-readable JSON
effigy contracts check-json --fast --print-selected=json
```

Use `--print-selected` when you need to know exactly which schemas were validated
in a given run.

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
- `binary.version` is the shipped semver; `binary.active_version` and
  `binary.display_version` can include local build identity such as
  `+local.<hash>`.
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
