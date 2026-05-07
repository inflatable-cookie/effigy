# 036 - Artifact Substrate For Seed Apply And Capture Workflows

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-06
Depends on: [`035-contract-promotion-public-cleanup-breaks-and-closeout.md`](./035-contract-promotion-public-cleanup-breaks-and-closeout.md)

## Goal

Define and implement a standalone Effigy artifact substrate for local and OCI
data payloads used by bootstrap, data seed, UAT apply, and UAT capture
workflows.

The first proof target is Acowtancy. Effigy should remove transport, staging,
metadata, and operation-reporting weight from Acowtancy while keeping
Acowtancy's migration and content-coercion logic app-owned.

## Scope

- add an artifact contract
- define `artifact`, not `bundle`, as the product term
- plan a standalone `effigy artifact ...` built-in ecosystem
- define `effigy-artifacts` crate boundary
- support local SQL payloads as artifact sources
- support OCI artifact refs as artifact sources
- integrate artifact sources with `bootstrap --db-seed`
- integrate artifact sources with `container data seed`
- integrate artifact destinations with `container data dump`
- define UAT apply/capture operation reports
- define a small artifact operation ledger model
- prove the Acowtancy handoff boundary

## Non-Goals

- no generic migration framework
- no MySQL-to-Postgres transform engine
- no Acowtancy schema logic in Effigy
- no production deployment orchestration
- no `.github/workflows/` edits
- no release execution

## Proposed Built-Ins

Standalone artifact surface:

```sh
effigy artifact inspect <REF|PATH>
effigy artifact pull oci://<REF>
effigy artifact stage <REF|PATH>
effigy artifact list
effigy artifact capture <SOURCE> --ref oci://<REF>
```

Seed and dump integrations:

```sh
effigy bootstrap <repo> --db-seed cbs=./backups/cbs.sql.gz
effigy bootstrap <repo> --db-seed cbs=oci://ghcr.io/acowtancy/legacy-cbs:2026-05-06
effigy container data seed --db-seed cbs=oci://ghcr.io/acowtancy/legacy-cbs@sha256:...
effigy container data dump --db-dump cbs=oci://ghcr.io/acowtancy/uat-content:2026-05-06
```

The `oci://` prefix is required in the first implementation round.

## Crate Boundary

Add `crates/effigy-artifacts`.

Responsibilities:

- parse artifact refs
- classify local file sources
- model artifact metadata
- stage local payloads
- define OCI pull/push requests and reports
- define artifact operation reports
- expose test builders for local and OCI-shaped fixtures

Dependencies should stay minimal. OCI implementation may use a proven crate or
external tool only after a focused evaluation card.

## Metadata Shape

First schema:

- `effigy.artifact.v1`

Required fields:

- schema
- kind
- source type
- source ref/path
- digest when known
- staged root
- primary files
- optional environment label for apply/capture reports

Artifact kinds should be coarse:

- `sql-dump`
- `legacy-source-snapshot`
- `migrated-base-snapshot`
- `uat-content-snapshot`
- `content-overlay`
- `app-specific`

Effigy must not interpret these kinds beyond routing, reporting, and validation
guards.

## UAT Requirements

Acowtancy artifacts must work from UAT.

Planning assumptions:

- Effigy can be installed on UAT as an operator tool
- Effigy is not required for normal request serving
- UAT registry auth comes from the deployment environment
- artifact refs are passed explicitly
- digest-pinned refs are preferred for audited apply operations
- Effigy records what artifact was applied or captured

The app owns DB-level idempotency and migration history. Effigy owns the outer
artifact operation record.

## Acowtancy Proof

The proof should inspect `~/Dev/projects/acowtancy` and identify:

- current OCI artifact production/consumption path
- current legacy MySQL snapshot shape
- app-local Rust binaries involved in migration
- where local SQL payload staging currently happens
- how UAT should apply a chosen artifact version
- how UAT-created content could be captured as a later artifact
- which path Effigy can replace first with minimal behavior change

The first implementation proof should be one vertical slice:

1. local SQL seed resolves through artifact staging
2. OCI-shaped fixture resolves through the same staging contract
3. existing seed task receives the resolved artifact path/context
4. apply report records artifact source and digest if available
5. capture planning can package a generated SQL dump

### Initial Acowtancy Audit Notes

Current Farmyard seed-bundle flow already has a useful app-owned split:

- `farmyard/scripts/tasks/seed-bundle-build.sh` packages generated
  `migration/dist/seed-bundles/*` directories into `.oci` files through the
  Underlay devtools seed-bundle command.
- `farmyard/scripts/tasks/seed-bundle-publish.sh` publishes built `.oci`
  files to the local OCI store with refs like
  `farmyard/seed-data/<name>:latest`.
- `farmyard/scripts/tasks/seed-bundle-install.sh` consumes either an
  `oci_ref` or a local `bundle_file` from `seed-bundles.sources.json`, pulls
  it into `migration/dist/seed-bundles/<name>`, then regenerates local
  post-SQL hook artifacts.
- `farmyard/seed-bundles.sources.sample.json` already models digest-pinned
  registry refs for `spine` and `content`.
- `farmyard/migration/dist/seed-bundles/bundle-set.json` is the app-owned
  replay manifest. It declares families, priority, replay hooks, and patch
  overlay hooks.
- `farmyard/.underlay-local-oci` is a local development OCI store used by the
  current devtools path.

The app-owned migration model is broader than Effigy should absorb. Farmyard
has bundle families such as `spine`, `content`, `exam-linkage`,
`exam-content`, `archive`, and `dev-fixtures`; post-SQL hook indexes; owner
scoped media request manifests; patch overlay reports; and closeout gates. The
Effigy artifact substrate should treat those as opaque app payloads and
operation metadata, not as built-in migration semantics.

First replacement candidate:

- Move the transport/staging half of `seed-bundle-install.sh` behind Effigy
  artifact staging.
- Preserve Farmyard's regeneration of post-SQL hook artifacts and
  `bundle-set.json` ownership.
- Let Farmyard continue deciding replay order, post-SQL handlers, media
  finalization, patch overlays, and residual queues.

UAT proof implication:

- UAT should receive explicit artifact refs, preferably digest-pinned.
- Effigy should record the resolved ref, digest, staging root, target
  environment label, and invoked app command.
- Farmyard should record app-level migration state and idempotency results.
- A capture flow should produce an explicit artifact plus a ledger entry, but
  publishing must remain an explicit operator action.

## Milestone Cards

### 415 - Plan Artifact Contract And Acowtancy Boundary

Status: Complete

Scope:

- promote `014-artifact-substrate-contract.md`
- audit Acowtancy migration/artifact flow
- define artifact metadata schema draft
- define local/OCI ref syntax
- define UAT apply/capture ledger fields
- define first proof fixture

Exit condition:

- Effigy/Acowtancy responsibility boundary is explicit enough to implement
  without moving app migration logic into Effigy.

Closeout:

- `014-artifact-substrate-contract.md` exists as the durable contract draft.
- Acowtancy audit notes identify the current Farmyard seed-bundle build,
  publish, install, local OCI, and `bundle-set.json` surfaces.
- The first replacement boundary is transport/staging for seed-bundle install,
  leaving Farmyard in charge of replay semantics.

### 416 - Scaffold Effigy Artifacts Crate

Status: Complete

Scope:

- add `crates/effigy-artifacts`
- parse local and `oci://` refs
- model artifact metadata
- model staged artifact reports
- add focused unit tests

Closeout:

- `crates/effigy-artifacts` is in the workspace.
- local refs, explicit `oci://` refs, artifact kinds, metadata, staging
  reports, and operation report shells are modeled.
- focused crate tests cover local SQL, `.sql.gz`, `.dump`, explicit OCI refs,
  and unprefixed registry-looking ref rejection.

### 417 - Local Artifact Staging For Seed Inputs

Status: Complete

Scope:

- stage local SQL payloads through `effigy-artifacts`
- preserve existing `--db-seed` behavior
- synthesize `effigy.artifact.v1` metadata
- wire bootstrap seed staging through the artifact resolver

Closeout:

- local SQL-like artifact staging exists in `effigy-artifacts`
- staging writes deterministic roots under `.effigy/local/artifacts`
- staging copies the primary payload and writes `effigy-artifact.json`
- focused crate tests cover metadata output, deterministic roots, and missing
  local source errors

### 418 - OCI Pull Inspect And Stage

Status: Complete

Scope:

- choose OCI implementation path
- implement inspect/pull for private registry refs
- capture digest
- stage OCI payloads into the same local artifact shape
- add no-token-logging tests

Closeout:

- `effigy-artifacts` now has OCI inspect/pull request and report models.
- OCI transport is behind an `OciArtifactAdapter` trait.
- reportable refs redact userinfo.
- digest-pinned refs populate descriptor digest fields.
- pulled OCI-shaped fixture payloads stage into the same
  `StagedArtifactReport` / `effigy.artifact.v1` metadata model as local
  payloads.

### 419 - Seed Dump Apply Capture Integration

Status: Complete

Scope:

- let `container data seed` consume artifact refs
- let `container data dump` write local or OCI artifact destinations
- add UAT-shaped apply/capture reports
- add artifact operation ledger draft

Closeout:

- local seed staging routes through `effigy-artifacts`
- the legacy `.effigy/local/db-seeds` handoff stays intact
- focused seed tests prove existing local SQL behavior still works

### 420 - Acowtancy Proof And Closeout

Status: Complete

Scope:

- prove one Acowtancy path end to end
- document UAT operator flow
- document remaining app-owned migration surfaces
- update docs and changelog for user-facing behavior

Closeout:

- Acowtancy/Farmyard proof confirms Effigy should replace only the
  transport/staging half of `seed-bundle-install.sh` first.
- Farmyard keeps `bundle-set.json`, family ordering, hook artifacts, patch
  overlays, residual queues, and migration idempotency.
- UAT apply/capture should use explicit refs, digest-pinned where possible,
  with Effigy recording outer artifact metadata and Farmyard recording
  app-level migration state.
- The next implementation round should add public `artifact inspect/stage` and
  Farmyard handoff output before live private-registry proof.

### 421 - Implement Artifact Inspect Stage And Farmyard Handoff

Status: Complete

Scope:

- add `effigy artifact inspect <REF|PATH>`
- add `effigy artifact stage <REF|PATH>`
- support local files and explicit `oci://` refs at the parsing/report layer
- stage local SQL-like payloads through `effigy-artifacts`
- emit JSON/text reports with metadata path, source, kind, staged root, primary
  files, and digest when known
- add optional Farmyard handoff output

Closeout:

- `effigy artifact inspect/stage` is wired through the public CLI, help,
  command envelope labels, and runner dispatch.
- local staging uses `effigy-artifacts` and writes `effigy-artifact.json`.
- explicit `oci://` refs inspect through the report layer while live transport
  remains behind the adapter boundary.
- Farmyard handoff JSON is stable enough for app-local adoption work.

### 422 - Live OCI Transport And Private Registry Proof

Status: Complete

Scope:

- choose and wire live authenticated OCI inspect/pull transport
- keep credentials redacted from logs and reports
- wire `effigy artifact inspect/stage oci://...` through the adapter boundary
- prove command behavior with fake transport fixtures before any real registry
  proof

Exit condition:

- OCI inspect/stage can use live authenticated transport, reports redact
  private-registry details, and fake-transport tests cover the command layer.

Closeout:

- live OCI inspect/pull uses the local `oras` CLI behind `OciArtifactAdapter`
- `artifact inspect/stage oci://...` is wired through the adapter boundary
- fake adapter tests cover command-layer inspect, pull, staging, metadata, and
  Farmyard handoff
- OCI userinfo is redacted from reportable refs, descriptors, staged metadata,
  and process errors
- the artifact contract documents UAT/private registry auth through `oras login`
  or equivalent registry-client auth

### 423 - Wire OCI Artifact Refs Into Seed And Dump Surfaces

Status: Complete

Scope:

- allow `bootstrap --db-seed <target>=oci://...`
- allow `container data seed --db-seed <target>=oci://...`
- preserve current local SQL seed behavior and legacy staged seed handoff
- decide the first bounded behavior for `container data dump <target>=oci://...`
- keep app migration semantics outside Effigy

Exit condition:

- seed flows can resolve OCI artifact refs through the same staged primary-file
  model as local SQL files, and dump behavior is either implemented for local
  artifact output or explicitly parked behind capture/push.

Closeout:

- bootstrap and container data seed preserve explicit `oci://` refs through path
  resolution
- shared seed staging pulls OCI artifacts through the adapter and copies the
  staged primary file into the legacy `.effigy/local/db-seeds` handoff
- local SQL seed behavior is unchanged
- dump-to-OCI is parked behind capture/push planning

### 424 - Plan OCI Capture Push For UAT Snapshots

Status: Complete

Scope:

- decide command shape for artifact capture and dump-to-OCI destinations
- define metadata, digest, tag, overwrite, and immutability rules
- decide UAT push authentication and authorization expectations
- decide whether dump-to-OCI writes directly or produces a local staged artifact
  first

Exit condition:

- capture/push behavior is specified tightly enough to implement without
  guessing about UAT safety, tag mutability, or app ownership.

Closeout:

- write-side command shape is defined for artifact capture and dump-to-OCI
- capture is two-phase by default: stage locally, push only with explicit intent
- digest-pinned refs are invalid push destinations
- pushed tags must report immutable digests
- UAT auth stays in registry-client auth stores
- Effigy owns outer artifact reports; Farmyard owns app-level snapshot validity

### 425 - Implement Local Artifact Capture With Planned OCI Push

Status: Complete

Scope:

- add `effigy artifact capture <SOURCE_PATH> --ref oci://<REF>`
- stage local source through the artifact metadata model
- report planned OCI destination without live push
- reject digest-pinned destination refs
- keep container dump integration and live push for later cards

Exit condition:

- local capture produces staged artifact metadata plus a planned OCI destination
  report and gives Farmyard enough output for a local snapshot handoff.

Closeout:

- `effigy artifact capture <SOURCE_PATH> --ref oci://<REF>` is available
- capture stages local payloads and reports planned OCI destination metadata
- `--kind`, `--environment`, and `--farmyard-handoff` are supported
- digest-pinned destination refs are rejected
- `--push` is rejected until live push lands

### 426 - Wire Planned Artifact Capture Into Container Data Dump

Status: Complete

Scope:

- accept `container data dump <TARGET>=oci://<REF>`
- write the SQL dump to a local staged source
- pass that source through artifact capture
- keep local file dump behavior unchanged
- do not mutate registries

Exit condition:

- dump-to-OCI produces the same planned capture report as artifact capture,
  local dump behavior still passes, and no registry mutation occurs.

Closeout:

- `container data dump <TARGET>=oci://<REF>` preserves OCI refs through output
  path resolution
- dump-to-OCI writes local SQL under `.effigy/local/data-dumps`
- JSON reports include the local dump path and planned artifact capture payload
- no registry push occurs

### 427 - Implement Live OCI Push Through Artifact Adapter

Status: Complete

Scope:

- extend `OciArtifactAdapter` with push
- implement explicit push through local `oras`
- wire `artifact capture --push`
- report immutable pushed digest
- keep credentials redacted

Exit condition:

- `artifact capture --push` publishes through the adapter boundary and fake
  transport tests prove no credentials leak.

Closeout:

- `OciArtifactAdapter` now exposes live push
- ORAS-backed push is wired into `artifact capture --push`
- capture still stages locally before push
- capture reports immutable pushed digest when available
- fake adapter tests prove command-layer push reporting

### 428 - Decide Container Data Dump Live Push Boundary

Status: Complete

Scope:

- decide whether `container data dump <TARGET>=oci://<REF>` should mutate
  registries directly or stay planned-only
- define required flags such as `--push` or `--overwrite`
- preserve explicit UAT auditability

Exit condition:

- the dump live-push boundary is explicit and the next implementation or stop
  condition is clear.

Closeout:

- initial decision kept `container data dump <TARGET>=oci://<REF>`
  planned-only.
- one-command dump-and-push was later implemented in card `430` with explicit
  `--push`.
- dump-and-push must not become an automation default.

### 429 - Close Artifact Substrate Lane

Status: Complete

Scope:

- confirm the durable contract carries the selected behavior
- mark this roadmap and strict lane complete
- update front doors so no stale ready card remains
- record that dump-and-push needs explicit planning before implementation

Exit condition:

- `g03.036` is closed and the next move is planning, not accidental
  implementation.

Closeout:

- this roadmap is complete
- strict lane `042` is complete
- no next implementation card remains in this lane

### 430 - Implement Container Data Dump Live OCI Push

Status: Complete

Scope:

- add explicit `--push` to `container data dump`
- keep local dump behavior unchanged
- publish only explicit `oci://` destinations
- reject local-only dumps with `--push`
- route publication through artifact capture push

Exit condition:

- `container data dump --db-dump <TARGET>=oci://<REF> --push` stages the dump,
  publishes it through the artifact adapter, and reports pushed metadata.

Closeout:

- one-command dump-and-push exists as explicit opt-in behavior
- no implicit registry mutation was added
- overwrite and credential management remain outside this round

## Acceptance Criteria

- local SQL seed inputs and OCI artifact seed inputs resolve through one model
- artifact metadata is stable and inspectable
- artifact refs do not use the word bundle
- artifact built-ins are top-level, not under `container`
- UAT apply/capture requirements are documented
- Acowtancy proof does not move migration logic into Effigy
- private artifact security assumptions are explicit

## Next Task

Stop in planning and choose the next roadmap deliberately.
