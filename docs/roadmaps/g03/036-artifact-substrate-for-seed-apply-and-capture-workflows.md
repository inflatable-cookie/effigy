# 036 - Artifact Substrate For Seed Apply And Capture Workflows

Generation: `g03`

Status: Planned
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

### 376 - Plan Artifact Contract And Acowtancy Boundary

Status: Ready

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

### 377 - Scaffold Effigy Artifacts Crate

Status: Blocked by 376

Scope:

- add `crates/effigy-artifacts`
- parse local and `oci://` refs
- model artifact metadata
- model staged artifact reports
- add focused unit tests

### 378 - Local Artifact Staging For Seed Inputs

Status: Blocked by 377

Scope:

- stage local SQL payloads through `effigy-artifacts`
- preserve existing `--db-seed` behavior
- synthesize `effigy.artifact.v1` metadata
- wire bootstrap seed staging through the artifact resolver

### 379 - OCI Pull Inspect And Stage

Status: Blocked by 378

Scope:

- choose OCI implementation path
- implement inspect/pull for private registry refs
- capture digest
- stage OCI payloads into the same local artifact shape
- add no-token-logging tests

### 380 - Seed Dump Apply Capture Integration

Status: Blocked by 379

Scope:

- let `container data seed` consume artifact refs
- let `container data dump` write local or OCI artifact destinations
- add UAT-shaped apply/capture reports
- add artifact operation ledger draft

### 381 - Acowtancy Proof And Closeout

Status: Blocked by 380

Scope:

- prove one Acowtancy path end to end
- document UAT operator flow
- document remaining app-owned migration surfaces
- update docs and changelog for user-facing behavior

## Acceptance Criteria

- local SQL seed inputs and OCI artifact seed inputs resolve through one model
- artifact metadata is stable and inspectable
- artifact refs do not use the word bundle
- artifact built-ins are top-level, not under `container`
- UAT apply/capture requirements are documented
- Acowtancy proof does not move migration logic into Effigy
- private artifact security assumptions are explicit

## Next Task

Start card `376-plan-artifact-contract-and-acowtancy-boundary`.
