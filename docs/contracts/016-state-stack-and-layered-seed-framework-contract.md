# 016 - State Stack And Layered Seed Framework Contract

Status: Active
Owner: Platform
Created: 2026-05-08

## Purpose

Effigy needs a standard framework for how a repo composes system state from
schema, baseline seed data, imported snapshots, overlays, and later captures.

The shipped artifact substrate already covers transport, staging, metadata, and
OCI/local parity. This contract sits above that substrate and defines the
ordered replay model.

The first proof target is Example App's legacy-to-new-site UAT loop, where:

- legacy data arrives through replayable OCI artifacts
- new-system-authored UAT changes must be captured without being lost
- refreshes from the still-live legacy site must be able to feed a rebuilt
  baseline later

Effigy owns the framework for that loop. The app still owns what the data means
and how conflicts are resolved.

## Scope

The state-stack framework owns:

- lifecycle phase taxonomy for seeded system state
- stack-manifest structure and ordered layer replay
- separation between layer orchestration role and artifact payload kind
- runtime plan/report shape for stack apply/capture operations
- lineage/provenance records for applied stacks
- app-hook boundaries for role-specific apply/capture work
- explicit UAT freeze/capture/rebase workflow support

The state-stack framework does not own:

- source-to-target transform logic
- schema-specific or record-level merge decisions
- domain validation
- media rewrite semantics
- post-go-live sync engines
- automatic background replication

## Core Terms

State stack:

- the ordered set of layers used to build or rebuild an environment

Layer:

- one declared state input in a stack
- may resolve from a repo-local path, an OCI artifact, or an app-produced
  capture output

Layer role:

- the orchestration meaning of a layer
- examples: `structure`, `baseline-seed`, `legacy-import`, `media-library`,
  `dev-overlay`, `uat-capture`, `full-capture`

Artifact kind:

- the coarse descriptive type of a payload
- examples: `sql-dump`, `migrated-base-snapshot`, `content-overlay`,
  `object-store`, `app-specific`

Layer role and artifact kind are different:

- role says where a layer sits in the replay lifecycle
- kind says what sort of payload was packaged

Lineage:

- the immutable record of what exact stack, layer refs, digests, and hooks were
  used to build an environment

Rebase:

- a deliberate rebuild path where newer imported legacy state and captured
  new-system-authored changes are reconciled into a new baseline

## Phase Taxonomy

Effigy recognizes this first phase model for the first shipped stack surface.

### 1. `structure`

- repo-committed schema baseline
- usually SQL migrations or equivalent schema-apply contract

### 2. `baseline-seed`

- repo-committed low-volatility data
- examples: roles, groups, lookup rows, platform invariants

### 3. `legacy-import`

- imported state derived from an external or legacy source
- usually replayable through local or OCI artifacts
- Effigy does not define the transform logic that produced it

### 4. `base-apply`

- the point where imported layers are applied onto a clean target system

### 5. `dev-overlay`

- optional local or non-production-only fixture layer
- must never be a dependency of imported baseline data

### 6. `working-baseline`

- checkpoint label, not necessarily a separate payload
- means the system is now usable for UAT or operator review

### 7. `uat-capture`

- captured changes authored inside the new system after the baseline was built

### 8. `legacy-refresh`

- newer imported legacy layer generated from a later source snapshot

### 9. `rebase`

- offline reconciliation plus rebuild path that combines captured new-system
  changes with refreshed imported legacy state into a new baseline

### 10. `schema-evolution`

- ordinary forward schema changes after the baseline already exists

### 11. `full-capture`

- whole-system capture for replay elsewhere

This taxonomy is ordered guidance, not a claim that every repo uses every phase.

First implementation rule:

- implementation may model every role now, but it should only execute
  `structure`, `baseline-seed`, `legacy-import`, `media-library`,
  `dev-overlay`, `uat-capture`, and `full-capture` until refresh/rebase command
  semantics are promoted by a later card

## Source Rules

Layer sources may be:

- repo-local path
- explicit `oci://` artifact ref
- staged local artifact id
- app-owned capture output promoted into a new layer

Effigy must keep source selection explicit. It must not infer registry refs from
Docker-like strings without `oci://`.

## Stack Manifest Rules

A repo-level stack manifest should declare:

- stack name
- target environment class
- ordered layers
- role for each layer
- source ref/path for each layer when static
- dependency edges when the order alone is not enough
- app hook identity for apply/finalize work when required
- production/dev eligibility
- provenance labels or snapshot identity when known

Minimum first-round stack fields:

- `schema`
- `name`
- `environment`
- `layers`

Minimum first-round layer fields:

- `key`
- `role`
- `source`
- `apply_mode`
- `environment_policy`

Recommended first-round layer fields:

- `depends_on`
- `artifact_kind`
- `snapshot_identity`
- `hook`
- `notes`

`hook` accepts either a task selector string or an inline task definition using
the same run-array syntax as `[tasks]`, for example:

```toml
hook = [{ rhai = "state/apply-media.rhai" }]
```

The initial schema name is `effigy.state-stack.v1`.

Valid first-round `apply_mode` values:

- `task`
- `artifact`
- `sql`
- `manual`
- `checkpoint`

Valid first-round `environment_policy` values:

- `all`
- `dev-only`
- `non-production`
- `production`
- `capture-only`

The first implementation should parse and validate this shape before it invokes
anything. A planning/reporting surface may land before live apply execution.

The manifest should describe composition, not app semantics. It can say a layer
is `legacy-import` or `uat-capture`; it must not pretend Effigy understands how
to reconcile two exam papers or two media references.

## Apply Rules

Stack apply should resolve into an explicit apply plan and report.

Core rules:

- `structure` runs before imported or captured data layers
- `baseline-seed` runs before imported overlays that depend on static rows
- `dev-overlay` runs only after the baseline is already working
- local and OCI sources resolve through the same artifact substrate before
  app-specific apply hooks run
- apply order must be visible in text and JSON reports

Apply report should include:

- schema id
- environment label
- stack identity
- resolved layers in order
- layer roles
- apply modes
- environment policy decisions
- refs/paths and digests where available
- invoked app hooks or tasks
- result per layer
- lineage id or lineage root
- timestamp

Current shipped boundary:

- `effigy state apply` emits `effigy.state-stack.apply.v1`
- `effigy state apply` is plan-only unless `--yes` is supplied
- task, artifact, and SQL apply modes have first-slice adapters
- capture, manual, checkpoint, and app-specific payload semantics remain
  unsupported in apply
- apply reports are persisted to the state report history layout

## Capture Report Boundary

State capture packages current system state into one or more replayable layers
plus lineage. It is a reporting and orchestration boundary, not a diff engine.

Capture must stay two-phase:

- capture stages local artifacts first
- OCI publish stays explicit through `--push`
- capture reports must identify the source environment and parent lineage
- produced layers must be appendable to a future state stack
- app-owned hooks may produce payloads, but Effigy records the outer operation

Capture modes:

| Mode | Intended Role | Meaning |
| --- | --- | --- |
| `uat-overlay` | `uat-capture` | Capture new-system-authored changes that should replay on top of a refreshed migrated baseline. This does not imply full database/media ownership. |
| `full-snapshot` | `full-capture` | Capture enough running state to recreate the current environment elsewhere. This may include database and media artifacts, but app-specific consistency rules remain repo-owned. |

Command shape:

```sh
effigy state capture --stack <NAME> --role uat-capture --source-env uat
effigy state capture --stack <NAME> --role full-capture --source-env uat
effigy state capture-set <STACK> <PROFILE>... [--key <KEY>] [--yes] [--push]
```

The first implementation is plan-only unless `--yes` is supplied, like
`state apply`.

`state capture-set` is an aggregate convenience surface for named capture
profiles. It runs each listed profile with the same capture key, generating a
timestamp key when `--key` is omitted. It does not change individual capture
semantics; each child capture still writes its normal capture report and
history entry. The aggregate capture-set report is also written to
`latest-capture-set.json` and a timestamped history file so an operator can
audit the grouped capture as one action.

Report schema:

- `schema`: `effigy.state-stack.capture.v1`
- `schema_version`: `1`
- `ok`
- `executed`
- `stack_name`
- `source_environment`
- `capture_role`
- `capture_mode`
- `parent_lineage_id`
- `created_at`
- `produced_layers[]`
- `capture_artifacts[]`
- `tasks[]`
- `warnings[]`

Capture-set report schema:

- `schema`: `effigy.state-stack.capture-set.v1`
- `schema_version`: `1`
- `ok`
- `executed`
- `stack`
- `key`
- `created_at`
- `profiles[]`
- `captures[]`
- `written_report_path`
- `written_history_path`

`captures[]` contains:

- `profile`
- `ok`
- `report` with the normal `effigy.state-stack.capture.v1` payload when the
  child capture reached report generation
- `error` when capture setup failed before a child report existed

`produced_layers[]` describes the state-stack layer material that the capture
would add or has added:

- `key`
- `role`
- `apply_mode`
- `environment_policy`
- `artifact_kind`
- `source_ref`
- `snapshot_identity`
- `depends_on[]`
- `hook`

`capture_artifacts[]` references artifact-level capture reports:

- `layer_key`
- `operation`: `planned-capture`, `captured-local`, or `pushed`
- `artifact_report`
- `digest` when pushed
- `ref` when an OCI destination is known

`tasks[]` records repo-owned capture hooks:

- `name`
- `status`: `planned`, `executed`, or `failed`
- `context_path` when a task context file was written
- `output`
- `error`

## Capture Task Context

Repo-owned capture tasks receive a stable context surface. Environment
variables remain convenience aliases, but the versioned JSON context file is the
preferred app integration seam because it can grow without creating shell
quoting or naming churn.

Context file:

- path: `.effigy/state/capture-context/<stack>/<key>.json`
- schema: `effigy.state-stack.capture-context.v1`
- written before the repo task runs
- overwritten by later captures with the same stack/key
- exposed to the task as `EFFIGY_STATE_CAPTURE_CONTEXT`
- recorded in the capture report as `tasks[].context_path`

Context fields:

- `schema`
- `schema_version`
- `stack_name`
- `parent_lineage_id`
- `capture_role`
- `capture_mode`
- `source_environment`
- `key`
- `source` when supplied
- `destination_ref` when supplied

Environment aliases:

- `EFFIGY_STATE_CAPTURE_SCHEMA`
- `EFFIGY_STATE_CAPTURE_STACK`
- `EFFIGY_STATE_CAPTURE_PARENT_LINEAGE_ID`
- `EFFIGY_STATE_CAPTURE_ROLE`
- `EFFIGY_STATE_CAPTURE_MODE`
- `EFFIGY_STATE_CAPTURE_SOURCE_ENV`
- `EFFIGY_STATE_CAPTURE_KEY`
- `EFFIGY_STATE_CAPTURE_SOURCE` when supplied
- `EFFIGY_STATE_CAPTURE_DESTINATION_REF` when supplied
- `EFFIGY_STATE_CAPTURE_CONTEXT` when a task context file is written

Path aliases are task-runtime paths. Relative capture sources are resolved to
absolute paths before task execution so repo tasks do not accidentally write to
the caller's current directory when `--repo` is used. The context JSON preserves
the manifest-level source value for reporting and reproducibility.

Rhai capture helpers:

- `state::capture_context()` returns the parsed capture context map
- `state::capture_context_path()` returns the context JSON path
- `state::capture_source()` returns the target payload path
- `state::capture_destination_ref()` returns the planned destination ref when
  supplied

Rhai tasks should prefer the `state` module over direct environment access.

App-owned semantics:

- selecting which rows/files belong in an overlay
- database/media consistency checks
- conflict detection
- record-level merge or reconciliation
- generating app-specific payloads

Effigy-owned semantics:

- validating the requested capture role and environment
- passing structured context to repo-owned tasks
- staging/publishing artifacts through existing artifact reports
- emitting the state-level capture report
- linking the produced layer back to parent lineage

Current shipped boundary:

- `effigy state plan` does not produce a capture report
- `effigy state apply` reports capture-shaped layers as unsupported
- capture-shaped roles may appear in a manifest for planning and lineage only
- `effigy state capture` emits the state-level capture report
- `effigy state capture --yes --source <PATH> --ref oci://...` stages an
  already-produced local payload and embeds `effigy.artifact.capture.v1`
- adding `--push` publishes the staged capture artifact to the explicit OCI ref
  and reports the digest
- `--task <TASK>` runs one repo-owned capture task before artifact staging and
  records `planned`, `executed`, or `failed`
- state capture does not run produced-layer apply hooks yet
- capture reports are persisted to the state report history layout

## Rebase Rules

Effigy should support the operator workflow for rebase, not the merge logic.

The bounded framework-owned flow is:

1. freeze the working environment
2. capture new-system-authored changes
3. generate a newer imported legacy layer
4. expose both inputs and their lineage clearly
5. rebuild from a clean baseline after app-owned reconciliation

Effigy must not claim to automatically reconcile conflicting domain rows.

## App Hook Boundary

Effigy needs app hooks because replaying layers often means more than piping SQL
into a database.

Framework-owned shape:

- Effigy resolves the layer source
- Effigy stages artifacts and computes lineage
- Effigy invokes the declared app hook with structured context. Hooks may be
  selector strings or inline task definitions in composed Effigy manifests.
- the app performs transform/apply/finalize logic
- Effigy records the outer operation report

App hook inputs should include:

- resolved staged paths
- layer role
- artifact kind
- source ref/path
- digest
- environment label
- parent lineage metadata when present

## Media Rules

Media may participate in the same stack through `role = "media-library"` and,
for S3-compatible object-store payloads, `artifact_kind = "object-store"`.
Media semantics stay app-owned.

Effigy may coordinate:

- ordered replay of media-bearing layers
- capture lineage
- staging of binary payloads
- object payload integrity and target reports once the object-store apply
  primitive exists

Effigy must not own:

- body rewrites
- attachment binding semantics
- per-domain media conflict resolution

## Lineage Rules

Lineage is a first-class outcome of the framework.

The current minimum lineage surface should record:

- stack name or id
- environment label
- ordered layers
- layer roles
- resolved refs/paths
- digests when available
- snapshot identities when declared
- invoked hooks/tasks
- timestamps

The first shipped surfaces are `effigy state plan`, `effigy state apply`,
`effigy state capture`, and `effigy state history`. Apply/capture reports share
the same environment-level lineage anchor. Effigy persists reports as
operator-readable files without introducing a database-backed ledger.

When no standalone manifest path is supplied, Effigy reads state config from the
effective composed manifest:

```toml
[state.uat]
schema = "effigy.state-stack.v1"
name = "example-app-uat"
environment = "uat"

[[state.uat.layers]]
key = "structure"
role = "structure"
source = "farmyard/db:migrate"
apply_mode = "task"
environment_policy = "all"
```

Large state declarations should use normal manifest composition, for example
including a repo-owned `state/example-app.state.toml` fragment from `effigy.toml`.
There is no separate state-specific file discovery convention.

Selection rules:

- `--manifest <PATH>` or positional `<MANIFEST>` plans a standalone
  `effigy.state-stack.v1` document
- no standalone manifest reads `[state]` from the composed manifest
- positional `<STACK>` or `--stack <NAME>` selects one composed stack explicitly
- `state.default` selects the default composed stack
- a single composed stack may be selected without `state.default`
- multiple composed stacks without `state.default`, positional `<STACK>`, or `--stack` fail with a
  clear ambiguity error

Named capture profiles:

```toml
[state.uat.captures.new-content]
role = "uat-capture"
source_env = "uat"
source = ".effigy/state/captures/{key}.tar"
ref = "oci://ghcr.io/example-app/state:{key}"
task = "state:capture-new-content"
```

`task` accepts either a task selector string or an inline task definition using
the same run-array syntax as `[tasks]`, for example:

```toml
task = [{ rhai = "state/capture-new-content.rhai" }]
```

`effigy state capture uat new-content --yes` expands the profile, defaults the
produced layer key to the profile name, and supports `{stack}`, `{profile}`, and
`{key}` in `source` and `ref` templates. Explicit CLI flags override profile
fields for one-off captures.

Lineage relationship:

- artifact operation reports remain the source of truth for individual artifact
  inspect, stage, capture, seed, and dump work
- state-stack plan reports include planned artifact-operation references by
  layer
- state-stack apply and capture reports embed concrete artifact operation
  reports by layer when an artifact operation occurs
- the state-stack lineage record is the environment-level rollup across all
  layers
- no database-backed ledger is required for the first history implementation

Minimum lineage report fields:

- `schema`
- `lineage_id`
- `stack_name`
- `environment`
- `created_at`
- `layers[]`
- `artifact_reports[]`
- `warnings[]`
- `written_report_path` when `--write-report` is used
- `written_history_path` when history output is written

Report writing:

- `effigy state plan --write-report` writes the lineage report to
  `.effigy/reports/state/<stack>/plan.json`
- plan report writing also updates `latest-plan.json` and a timestamped
  `history/*.json` entry
- apply and capture report-producing commands update their matching latest file
  and a timestamped `history/*.json` entry
- the written report is the same lineage payload returned through stdout, with
  `written_report_path` and `written_history_path` set where applicable
- report writing does not make the report an append-only ledger
- the report path is an operator artifact, not proof that any layer has been
  applied

## State Report History

State history should be file-based first. The goal is to let operators find
prior plan/apply/capture reports without creating a hidden database or making
the report directory authoritative beyond the files it contains.

Directory shape:

```text
.effigy/reports/state/<stack>/
  latest-plan.json
  latest-apply.json
  latest-capture.json
  history/
    <created-at>-plan-<short-lineage>.json
    <created-at>-apply-<short-lineage>.json
    <created-at>-capture-<short-lineage>.json
```

Rules:

- latest files are convenience pointers and may be overwritten
- `history/` files are append-only by convention
- file names must be filesystem-safe and lexically sortable
- `<created-at>` should use UTC basic timestamp form when execution creates the
  report, for example `20260508T143012Z`
- `<short-lineage>` should be derived from the report lineage id or parent
  lineage id and truncated to a readable safe component
- report content remains the source of truth; file names are lookup indexes
- manual file deletion is allowed and should not corrupt hidden state
- history lookup must tolerate missing latest files, missing history files, and
  mixed old/new report layouts

Report kinds:

| Kind | Schema | Lineage Field |
| --- | --- | --- |
| `plan` | `effigy.state-stack.lineage.v1` | `lineage_id` |
| `apply` | `effigy.state-stack.apply.v1` | `lineage_id` |
| `capture` | `effigy.state-stack.capture.v1` | `parent_lineage_id` plus produced layer keys |

Lookup semantics:

- default lookup lists newest reports for the selected stack
- `--kind plan|apply|capture` narrows by report kind
- `--limit <N>` bounds output
- `--lineage <ID>` matches `lineage_id` or `parent_lineage_id`
- `latest-*` files can speed up lookup but must not be the only source
- history lookup should read JSON reports directly and ignore malformed files
  with warnings rather than failing the whole query

Command shape:

```sh
effigy state history uat
effigy state history uat --kind capture --limit 5
effigy state history uat --lineage <ID>
```

Minimum history payload:

- `schema`: `effigy.state-stack.history.v1`
- `schema_version`: `1`
- `stack_name`
- `reports[]`
- `warnings[]`

Each `reports[]` entry should include:

- `kind`
- `schema`
- `path`
- `created_at`
- `lineage_id` when available
- `parent_lineage_id` when available
- `ok` when available
- `executed` when available
- `summary`

Smallest implementation slice:

- keep existing `plan.json` write for compatibility
- add timestamped history writes for plan/apply/capture reports
- add latest pointers for plan/apply/capture reports
- add a read-only `state history` command that scans files
- do not introduce retention, pruning, compaction, or a database index yet

## Apply Adapter Boundary

The first execution adapter started task-only and now includes artifact staging
and SQL import.

`effigy state apply` without `--yes` renders an apply plan and does not execute
layers. `effigy state apply --yes` executes layers with `apply_mode = "task"`
by passing each layer `source` to existing Effigy task execution in stack order.
It stages layers with `apply_mode = "artifact"` through the existing artifact
staging substrate and embeds the resulting `effigy.artifact.stage.v1` report.
It imports layers with `apply_mode = "sql"` through the existing database
seed/import path.

Rules:

- `task` layers execute through repo-owned tasks
- `artifact` layers stage only; they are not applied to databases or media
- `sql` layers stage their SQL payload as an artifact, copy the staged primary
  file into the DB seed handoff directory, and run the existing DB seed/import
  task path
- capture, manual, checkpoint, and app-specific payload semantics are reported
  as `unsupported`
- task output is captured in `effigy.state-stack.apply.v1`
- artifact staging reports are captured in `effigy.state-stack.apply.v1`
- SQL import reports are captured in `effigy.state-stack.apply.v1` as
  `effigy.state-stack.sql-import.v1`
- task failure marks the layer `failed`, sets `ok = false`, and stops further
  task execution
- app-specific behavior remains inside repo-owned tasks
- capture and conflict logic remain future adapters

## SQL Apply Adapter

SQL apply is a database import adapter, not a migration engine.

The first SQL boundary reuses Effigy's existing data target model:

- `[data.targets.<name>]` remains the named database target contract
- bundle database declarations remain valid implicit targets
- target resolution must use the same target-selection semantics as database
  seed/import flows
- supported database engines are the same engines already supported by
  generated-compose data seed/import

`apply_mode = "sql"` should mean:

- the layer source resolves to a SQL payload
- local and OCI SQL payloads stage through the artifact substrate first
- the staged primary SQL file is imported into one explicit data target
- the import report is embedded in `effigy.state-stack.apply.v1` under
  `sql_report`
- no app-specific transform, merge, or validation logic runs inside Effigy

Target selection must be explicit enough to avoid destructive ambiguity:

- if the repo declares exactly one database target, SQL layers may use it by
  default
- if multiple database targets exist, each SQL layer must declare a target
  before execution
- `target = "<name>"` is accepted as the layer-level SQL target field and maps
  to the internal `sql_target` lineage field
- missing or unknown targets must fail before any SQL layer executes

Safety gates:

- `effigy state apply` remains plan-only by default
- `effigy state apply --yes` is required for SQL execution
- SQL target selection is preflighted before any state layer executes
- SQL execution uses import semantics that match existing database seed behavior
  rather than inventing a new SQL runner

Report shape additions for SQL layers:

- `target`: selected logical data target
- `sql_report.artifact_reports`: staged SQL artifact report list
- `sql_report`: database import result
- `status`: `would-import`, `planned-sql-import`, `imported`, or `failed`

## Example App Proof Boundary

The first proof case is the UAT freeze/rebuild loop:

1. apply `structure`
2. apply `baseline-seed`
3. apply `legacy-import`
4. optionally apply `dev-overlay`
5. freeze UAT
6. capture `uat-capture`
7. regenerate `legacy-refresh`
8. reconcile offline in app-owned logic
9. rebuild a new baseline

Effigy should prove the orchestration and lineage around that loop.

First implementation slice:

- parse a stack manifest with Example App-shaped layers
- validate role/order/environment policy
- resolve local and `oci://` artifact sources through planning stubs where
  live transport is not needed
- emit a lineage report showing the ordered stack and artifact-operation
  references
- execute declared layer hooks after successful task execution, artifact
  staging, or SQL import using a structured apply-context handoff

Effigy should not absorb:

- MySQL snapshot transform logic
- content-specific conflict rules
- legacy/new media reference reconciliation

## Deferred For Later

- durable persisted cross-run lineage ledger
- post-go-live sync surfaces
- implementation-specific hook payload schema
- live rebase execution

## Next Task

Use this contract as the next release boundary. Further rebase execution
semantics should be driven by Example App rebasing real migration code onto the
released state-stack surface.
