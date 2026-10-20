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
effigy --json <selector> --plan
effigy --json <catalog-or-root-task>
```

When JSON mode is active, CLI preamble output is suppressed and output is pure JSON.
`effigy changelog extract` also omits the presentation banner when stdout is not a
terminal, so redirected release notes contain only the extracted changelog.
`--json` is an output format, never a planning or safety flag:
`effigy --json <selector>` still runs the task. Use `effigy --json <selector> --plan`
to inspect the resolved command without executing it.

For QA groups, prefer the global form for automation: `effigy --json tasks
qa-groups list` and `effigy --json tasks qa-group ...` use the canonical
`effigy.command.v1` envelope. The local `--json` flags on group list, run
(including `--plan`), and status also suppress the banner, but emit the
corresponding `effigy.qa-*` payload directly. `tasks qa-group logs` has no local
JSON form; use global `--json` to wrap its text result.

Task and draft text runs keep the framed header off stdout. Presentation goes to
stderr so command substitution receives only the task bytes.


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
effigy --json graph explore --excerpts-only "trace release orchestrator"
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

### Failed Task Runs Keep Their Output Payload

A failed task run does not drop captured task output. The JSON runner captures
the task's stdout and stderr (including Rhai script output written before the
failure), renders the normal `effigy.task.run.v1` payload, and attaches it to
`error.details` while the top-level envelope stays failed (`ok: false`, non-zero
exit). Automation reads captured diagnostics from
`error.details.stdout`/`error.details.stderr` instead of scraping terminal
text. Capture honors
existing redaction; secrets are removed before the payload is rendered.

## Result Payload Schemas

`result` (or `error.details` for some failures) contains command-specific schemas.

Current command payload schemas include:

Payload examples guide: `026-json-payload-examples.md`

- `effigy.help.v1`
- `effigy.tasks.v1`
- `effigy.tasks.filtered.v1`
- `effigy.tasks-status.v2`
- `effigy.tasks-status-all.v2`
- `effigy.deps.status.v1`
- `effigy.deps.link.v1`
- `effigy.deps.unlink.v1`
- `effigy.deps.pin.v1`

Operator workflow and recovery: [`077-local-dependency-linking.md`](077-local-dependency-linking.md).
- `effigy.doctor.v1`
- `effigy.doctor.explain.v1`
- `effigy.version.v1`
- `effigy.release.status.v1`
- `effigy.scan.god-files.v1`
- `effigy.scan.boundary-violations.v1`
- `effigy.scan.dead-code.v1`
- `effigy.scan.validation-gaps.v1`
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
- `effigy.graph.index.v1`
- `effigy.graph.status.v1`
- `effigy.graph.search.v1`
- `effigy.graph.node.v1`
- `effigy.graph.explore.v1`
- `effigy.graph.affected.v1`
- `effigy.docs.context.v1`
- `effigy.docs.context.sources.v1`
- `deploy.model.v1`
- `effigy.deploy.export.v1`
- `effigy.deploy.plan.v1`
- `effigy.deploy.apply.v1` (v0.15 apply/redeploy refusal in `error.details`)
- `effigy.deploy.status.v1`
- `effigy.deploy.history.v1`
- `effigy.init.v1`
- `effigy.init.list.v1`
- `effigy.init.checklist.v1`
- `effigy.init.actions.v1`
- `effigy.lock-wait.v2` (lock-conflict `error.details`)
- `effigy.completion.v2`
- `effigy.completion.candidates.v1`
- `effigy.task.run.v1`
- `effigy.task.plan.v1`
- `effigy.service.list.v2` (`sources` in precedence order; each fragment has `name`, `qualified`, `source`, `layer`, `path` and `shadows`)
- `effigy.service.extract.v1`
- `effigy.artifact.inspect.v1`
- `effigy.artifact.stage.v1`
- `effigy.artifact.capture.v1`
- `effigy.container.hosts.v1`
- `effigy.container.retire.v1`
- `effigy.container.retire-dead.v1` (`container retire --dead --json`)
- `effigy.container.ports.v1` (`container ports status|reclaim`)
- `effigy.container.bounded-exec.v1` (`container <NAME> shell --bounded --json`)
- `effigy.container-partial-activation.v2` (detached `container up` failure details)
- `effigy.runs.v1` (`runs --json`)
- `effigy.run.v1` (`run status|stop --json`)
- `effigy.run.logs.v1` (`run logs --json`)
- `effigy.admission.v1` (error details when a heavy run is not admitted)
- `effigy.admission-request.v1` (the `EFFIGY_ADMISSION_REQUEST` hook input)
- `effigy.state-stack.lineage.v1`
- `effigy.state-stack.apply.v1`

In v0.15, `deploy apply` and `deploy redeploy` exit nonzero with the existing
`effigy.command.v1` envelope, `ok: false`, and `error.details.reason` set to
`not_supported`. Supplying `--yes` does not enable execution. Full execution
design is in the deploy lane (Q-003); see [guide 074](074-deployment-guide.md).

Task inventory JSON always includes public and internal tasks. Catalog task
rows add `visibility`, `visibility_reason` and `group`; `--all` does not change
JSON. Internal tasks use ordinary task-run and task-plan schemas.

Examples:

```bash
effigy --json tasks
effigy --json tasks --task test
effigy --json tasks --resolve catalog-a/api
effigy --json tasks status test
effigy --json tasks status --all
effigy --json tasks --all
effigy --json provider-smoke
effigy --json doctor
effigy --json doctor --deep --catalog api
effigy --json doctor --repo /path/to/workspace catalog-a/build --watch
effigy --json scan god-files
effigy --json scan duplicate-blocks
effigy --json scan comment-ratio
effigy --json scan generated-assets
effigy --json scan generated-in-src
effigy --json scan attention-markers
effigy --json scan stale-suppressions
effigy --json service list
effigy --json config path
effigy --json config get containers.backend
effigy --json config --schema --target test
effigy --json graph index
effigy --json graph status
effigy --json graph search release --limit 10
effigy --json graph explore "trace deploy provider export" --max-files 6 --max-bytes 12288
effigy --json graph explore --excerpts-only "trace deploy provider export" --max-files 8 --max-bytes 4096
effigy --json graph affected src/runner/graph_command.rs --depth 2
effigy --json deploy model --repo /path/to/workspace
effigy --json deploy export <PROVIDER> --repo /path/to/workspace --path infra/deploy --plan
effigy --json deploy plan uat --repo /path/to/workspace
effigy --json deploy history uat --repo /path/to/workspace
effigy --json test --plan
effigy --json test
effigy --json init --dry-run
effigy --json config completion bash --export
effigy --json config completion candidates --prefix farm
effigy --json state plan ./state/example-app-uat.toml
effigy --json state plan uat --write-report
effigy --json state apply uat
effigy --json state capture uat new-content
effigy --json state capture uat --role uat-capture --source-env uat --key uat-capture-2026-05-08
effigy --json state history uat --kind capture --limit 5
effigy --json --repo /path/to/workspace build
effigy --json --plan --repo /path/to/workspace build
```

## Service List

`effigy.service.list.v2` replaces v1 in v0.15. v1's `source` labels
(`bundled`, `project-local (...)`, `installed-pack (...)`) and its pack
`selection` object are gone with the catalog-pack runtime. `effigy container
render --json` adds a `services` array naming each environment service's
`catalog`, `source`, `layer` and `path`.

## Graph Workflow Notes

Graph uses one database for the checkout. The command envelope has no catalog
selection or fan-out fields. Data queries refresh stale files lazily, while
plain status is read-only. `freshness.state` includes `ready`,
`refresh-recommended`, `degraded`, `missing-index`, `stale-index` and
`refreshing`. Gate on `usable`; stale and refreshing data are not current.
A concurrent writer is identified in `freshness.lock` and its committed-file
progress is reported as `refreshing (n/N)`.

`effigy.graph.node.v1` adds `target`, `direction`, `files` and `symbols` beside
its seed `file`/`symbol`, edges, references, diagnostics and freshness.
`--in` and `--out` select relationship direction; default is `both`.

`effigy.graph.explore.v1` has two payload modes: `navigation` keeps primary
owners, excerpts, relations, edit targets and likely tests. `excerpts-only`
keeps ranked `items`, `request`, `overflow`, `freshness` and `notes`.
Both modes use the same file, byte, language and path budgets.

File updates commit graph, FTS and per-file freshness keys together. There is
no snapshot copy or graph event stream. A cooperative timeout stops work at
a safe boundary and returns `effigy.graph.timeout.v1` with the last phase,
`done`, `total` and `committed: true`; counts can be null before indexing.
Retries keep committed progress. `--stale-index` skips refresh and lock wait.
Affected packets narrow likely validation and do not prove exhaustive test
reachability. See the [graph guide](076-code-graph-and-agent-workflows.md).

## Payload Examples

See `026-json-payload-examples.md` for realistic sample responses for each schema.

### Tasks selector inventory (`selectors`)

`effigy.tasks.v1` keeps `catalog_tasks`, `managed_profiles`, and
`builtin_tasks` stable and adds one additive `selectors` array, so tooling
can inventory every runnable task with a single query instead of flattening
unrelated envelope values:

```bash
effigy --json tasks | jq -r '.result.selectors[].selector'
effigy --json tasks | jq '.result.selectors[] | select(.kind == "builtin")'
```

Each entry carries:

- `selector`: the invocation-ready name (`effigy <selector>`). Catalog tasks
  use bare names at the root and root-qualified names (`<catalog>/<task>`)
  for nested catalogs; managed profiles use `"<task> <profile>"`; builtins
  use bare builtin names.
- `kind`: `task`, `managed-profile`, or `builtin`.
- `source`: `catalog` or `builtin`.
- `catalog`: the catalog alias for catalog-backed entries, `null` for builtins.

Display-only rows (for example empty catalogs with no task) contribute no
selector, deferred builtins stay excluded, and names are unique and
`selectors`-ordered deterministically (catalog entries first, builtins last).
Existing `catalog_tasks`, `managed_profiles`, and `builtin_tasks` consumers
are unaffected.

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
| Runner payload contracts | Command-specific payload schema/shape (for example `effigy.init.v1`, `effigy.task.run.v1`) | `src/tests/json_contract_tests.rs` |
| CLI envelope contracts | Top-level `effigy.command.v1` envelope, `command.kind/name`, and error wrapping/remediation propagation | `tests/cli_output_tests.rs` |

Rule of thumb:
- Add payload/schema assertions in `json_contract_tests`.
- Add envelope/wrapping assertions in `cli_output_tests`.
- Keep behavior/runtime semantics in `src/tests/runner_tests.rs`.

## Compatibility Notes

- `schema_version` is the top-level envelope version.
- `binary.version` is the shipped semver; `binary.active_version` and
  `binary.display_version` can include local build identity such as
  `+local.<hash>` from `EFFIGY_ACTIVE_VERSION` or the installed sibling
  `.active-version` stamp. Without either, they use the package version;
  discovery never runs Git to infer an unstamped development identity.
- Each binary metadata object samples active identity once and derives its
  display form from that observation. The version command shares that same
  snapshot between its result and outer envelope. A later command may observe
  a changed stamp; identity is not cached across commands.
- New optional fields may be added in `v1` without removing existing keys.
- Release gate JSON (`effigy.release.gates.v1`, `effigy.release.status.v1`,
  `effigy.release.prepare.v1`, and the matching plan/simulate payloads) may
  include additive optional `log_path` on each gate result and
  `environment_path` for the run. Named hosted reuse may add optional
  `reused`, `hosted_run_url`, `hosted_head_sha`, and `hosted_repository` on a
  gate result, and `gates.reused_gates` on execute/resume payloads. Progress
  and configured-gate inventory stay on stderr and never on JSON stdout.
  Schema ids are unchanged.
- Breaking envelope changes require a new top-level schema/version.

## Expected Outcome

After this guide, you should be able to:

- identify the stable envelope Effigy uses for machine consumers
- know where command-specific payload data lives inside the envelope
- choose the right validation path when a JSON contract changes

## Related Guides

- Init command contracts: [`019-init-foundation.md`](019-init-foundation.md)
- DAG/policy/locking behavior: [`020-dag-lock-policy-baseline.md`](020-dag-lock-policy-baseline.md)
- CI automation patterns: [`024-ci-and-automation-recipes.md`](024-ci-and-automation-recipes.md)

## Next Step

After updating any envelope or payload shape, add or refresh examples in [`026-json-payload-examples.md`](026-json-payload-examples.md) and run `effigy contracts check-json --fast`.


## Sequence results

Host sequences retain `effigy.task.run.v1` inside the command envelope and add
`steps` (index, label, child run ID, typed outcome, exit code and captured streams)
and `failed_step`. Later steps report `state: "not_started"` after failure.
Execution and exit codes match text mode, including 124 for timeout and 128 + n
for cancellation. A failed command envelope carries the task payload in
`error.details`; success carries it in `result`. This is an additive v1 extension.
Task status v2 adds nested `steps` to run views. Container steps are journalled
with their legacy exit status; remote custody is deferred to #236.
