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

An explicit `--json` — global or command-local — is the machine contract, so it
always emits one `effigy.command.v1` document. That holds wherever the
invocation runs: a plain shell, a task, a managed run, a container handoff or a
host-process workload. The internal nested-invocation marker a parent sets
(`EFFIGY_INTERNAL_SUPPRESS_HEADER`) hides the header and the retired-config
notice only; it never changes the output schema, and a workload that inherited
the marker from its supervisor still reads the envelope.

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
  "warnings": [],
  "error": null
}
```

`warnings` is always present and holds human-readable notices that explain
something the command accepted instead of refusing: today, the retired v0.15
manifest config this invocation read (see
[guide 085](085-v0.15.0-consumer-migration.md)). A `--json` run never writes
those notices to stderr, so a machine reader gets one parseable document.

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
  "warnings": [],
  "error": {
    "kind": "refused",
    "type": "RunnerError",
    "message": "...",
    "details": {}
  }
}
```

`error.kind` is the three-kind classification a person reads too: `refused`
(a rule said no), `unavailable` (something did not answer, and retrying once it
does is safe) or `broken` (an Effigy bug). `error.type` names the error's own
type (`RunnerError`, `CliParseError`). Read `error.kind` to decide whether a
retry can help; read `error.human` for the sentence and next action.

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
- `effigy.tasks-status.v3`
- `effigy.tasks-status-all.v3`
- `effigy.deps.status.v1`
- `effigy.deps.link.v1`
- `effigy.deps.unlink.v1`
- `effigy.deps.pin.v1`

Operator workflow and recovery: [`077-local-dependency-linking.md`](077-local-dependency-linking.md).
- `effigy.doctor.v1`
- `effigy.doctor.explain.v1`
- `effigy.version.v1`
- `effigy.release.status.v1`
- `effigy.release.plan.v1`
- `effigy.release.run.v1` and `effigy.release.receipt.v1` (`release run`,
  `release resume`, `release approve`, `release decide`, `release receipt`)
- `effigy.release.input.v1` and `effigy.release.output.v1`: file contracts a
  `task` target, artifact task or verify task reads at `$EFFIGY_RELEASE_INPUT`
  and writes at `$EFFIGY_RELEASE_OUTPUT` (contract 054 §6). They are registered
  in the schema index under `payload_schemas`; no `effigy` command emits them,
  so they have no runnable index row.
- `effigy.scan.god-files.v1`
- `effigy.scan.duplicate-blocks.v1`
- `effigy.config.v1`
- `effigy.bundle.sync.v1`
- `effigy.test.plan.v1`
- `effigy.test.results.v1`
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
- `effigy.task.plan.v1` (adds `shadowed_builtin: {name, escape}` when a
  repository task owns a built-in's word, so a plan can name the built-in's
  explicit spelling)
- `effigy.service.list.v2` (`sources` in precedence order; each fragment has `name`, `qualified`, `source`, `layer`, `path` and `shadows`)
- `effigy.service.extract.v1`
- `effigy.artifact.inspect.v1` (`state artifact inspect --json`)
- `effigy.artifact.stage.v1` (`state artifact stage --json`)
- `effigy.artifact.capture.v1` (`state artifact capture --json`)
- `effigy.container.restart.v1` (`container <NAME> restart --json`)
- `effigy.container.hosts.v1`
- `effigy.container.retire.v1`
- `effigy.container.retire-dead.v1` (`container retire --dead --json`)
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
effigy --json service list
effigy --json config path
effigy --json config get containers.backend
effigy --json config --schema --target test
effigy --json deploy model production --repo /path/to/workspace
effigy --json deploy export <PROVIDER> production --repo /path/to/workspace --path infra/deploy --plan
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

## Payload Examples

See `026-json-payload-examples.md` for realistic sample responses for each schema.

### Task rows (`effigy.tasks.v1`)

`effigy.tasks.v1` catalog task rows carry the task's optional one-line
`description` beside `visibility`, `visibility_reason` and `group`. The field is
always present and `null` when the task declares none, so a reader can rely on
its shape without a version bump; filtered payload `matches` rows carry it the
same way. `effigy --json <task> --help` renders the existing `effigy.help.v1`
payload with `topic` set to the task selector and the task's group,
description, run preview and manifest in `text`; nothing runs. The row field is
an additive change within `effigy.tasks.v1`, and task help reuses an existing
schema, so neither version moves.

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
- `effigy --json scan god-files` is the raw scanner payload. Use it when you need the full findings list, scan-local text snapshot, or report-output metadata. It reports `thresholds_source` (`configured` or `language-defaults`) and each finding's `language`.
- `effigy --json scan duplicate-blocks` is the raw duplication payload. Use it when you need normalized block spans, occurrence locations, and snippet fingerprints without doctor normalization. It stays opt-in for doctor.

## Contracts Validation Workflow

Use `effigy contracts` when you need to validate JSON contract artifacts or check
that Effigy's own command payloads still conform to declared schemas. The schema
index and the selection contract are declared by the project, not by Effigy:

```toml
[contracts]
schema_index = "docs/knowledge/contracts/json-schema-index.json"
selection_contract = "docs/knowledge/contracts/json-selection-contract.json"
```

`--index` and `--contract` override those paths. A project that declares neither
is refused with "no JSON schema index configured" rather than being checked
against Effigy's own documentation tree.

### The check

```bash
# Validate every declared schema, its envelope and its payload schema
effigy contracts check-json

# Check only schemas touched since a git ref (great for PRs)
effigy contracts check-json --changed-only origin/main
```

There is one behaviour, not a fast and a full one. A row that names a
`payload_schema` is validated against that schema, and an id the index's
`payload_schemas` map does not carry fails the check instead of being skipped.
Use `--changed-only` in CI to keep PR checks short.

### Validate the selection payload

The selection payload is the `{"selected":...}` line that
`--print-selected=json` prints. `check-json` validates it against the project's
selection contract with the same JSON Schema engine it uses for payloads:

```bash
# Validate this run's selection payload as part of the check
effigy contracts check-json --print-selected=json
# equivalent, without printing it
effigy contracts check-json --validate-selection

# Validate a selection payload another job stored
effigy contracts check-json --validate-selection --artifact ./contracts-selected.json
```

A violation is reported as a failure row for
`effigy.json-contract-selection.v1` in the `effigy.contracts.check-json.v1`
envelope, so the command still fails with an actionable, structured report.
`--print-selected=json` implies `--validate-selection`. A payload schema or
selection contract that is not valid JSON Schema is refused at admission as
`effigy.json-contract-body.v1` in that same envelope; no indexed command runs.

JSON Schema cannot compare a property with an array length, so `count` is a
producer guarantee: it is derived from `selected.length`. Everything else —
required keys, string-only `selected` and `changed_only_base`
nullability — is schema-checked.

### Print selected schemas

```bash
# Human-readable list
effigy contracts check-json --print-selected

# Machine-readable JSON
effigy contracts check-json --print-selected=json
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
  `effigy.release.plan.v1`, `effigy.release.prepare.v1` and the matching plan
  payloads) may
  include additive optional `log_path` on each gate result and
  `environment_path` for the run. Named hosted reuse may add optional
  `reused`, `hosted_run_url`, `hosted_head_sha`, and `hosted_repository` on a
  gate result, and `gates.reused_gates` on execute/resume payloads.
  `effigy.release.gates.v1` also carries `gates[]` (name, stage, command, task,
  description, `reuse_hosted_evidence`) as the configured inventory it ran, so
  a JSON reader sees the same gates the progress lines named. Streaming
  progress lines stay on stderr and never on JSON stdout. Schema ids are
  unchanged.
- `effigy.release.plan.v1` is the `effigy release --plan` payload (contract 054
  §5), replacing the retired simulate payload. It reports
  `identities[]` (name, kind, current and next version, tag, `follows`,
  version file), `files[]`, `gates[]` with their `stage`, `artifacts[]`,
  `targets[]`, `approvals.checkpoints`, `working_tree_clean` (`null` outside a
  git work tree), and `blockers[]`. `changelog.used` is false for a release
  with no `semver` identity, which never reads a changelog; `valid` is vacuous
  there. `effigy.release.status.v1` gains the same
  identity, plan-summary and `run` fields additively; `current_version` and
  `version_source` are omitted when the release has no `semver` identity.
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

After updating any envelope or payload shape, add or refresh examples in [`026-json-payload-examples.md`](026-json-payload-examples.md) and run `effigy contracts check-json`.


## Sequence results

Host sequences retain `effigy.task.run.v1` inside the command envelope and add
`steps` (index, label, child run ID, typed outcome, exit code and captured streams)
and `failed_step`. Later steps report `state: "not_started"` after failure.
Execution and exit codes match text mode, including 124 for timeout and 128 + n
for cancellation. A failed command envelope carries the task payload in
`error.details`; success carries it in `result`. This is an additive v1 extension.
Task status v2 adds nested `steps` to run views. Container steps run the same
way under remote custody and report the workload's own exit code, including a
built-in `exec` step, whose `stdout` and `stderr` are the command's streams.
