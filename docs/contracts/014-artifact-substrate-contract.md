# 014 - Artifact Substrate Contract

Status: Draft
Owner: Platform
Created: 2026-05-06

## Purpose

Effigy needs a standalone artifact substrate for versioned data payloads used
by local bootstrap, UAT data apply/capture, and later production cutover
workflows.

This contract deliberately uses `artifact`, not `bundle`. In Effigy, `bundle`
already means config bundle. Artifact means a versioned payload that may be a
local file or an OCI registry object.

The first proof target is Acowtancy legacy data migration, where app-specific
Rust binaries must keep migration/coercion logic, while Effigy owns transport,
staging, metadata, and invocation context.

## Scope

The artifact substrate owns:

- artifact reference parsing
- local file classification
- OCI reference parsing
- OCI pull and push planning
- digest capture and optional digest pinning
- local artifact staging
- artifact metadata shape
- artifact apply/capture operation reports
- integration with seed/dump command inputs
- deterministic handoff into task execution

The artifact substrate does not own:

- MySQL-to-Postgres migration logic
- app schema interpretation
- content coercion
- domain validation
- production deployment orchestration
- public web runtime behavior

## Terminology

Artifact:

- a versioned payload Effigy can resolve, stage, inspect, apply, or capture

Artifact source:

- local file path
- OCI registry reference
- staged local artifact id

Artifact kind:

- coarse metadata for human and automation routing
- examples: `sql-dump`, `legacy-source-snapshot`, `migrated-base-snapshot`,
  `uat-content-snapshot`, `content-overlay`, `app-specific`

Artifact apply:

- a controlled operation that stages an artifact and invokes a declared app
  task or command against a target environment

Artifact capture:

- a controlled operation that packages generated data output into a new local
  or OCI artifact

Artifact ledger:

- durable operation record naming what artifact was applied or captured, with
  digest, environment, task, timestamp, and result

## Source Rules

Supported first-round sources:

- local `.sql`
- local `.sql.gz`
- local `.dump`
- OCI refs with an explicit `oci://` prefix

`oci://` is required in the first round. Effigy must not guess whether a
Docker-like string is a local path, registry ref, or app argument.

Examples:

```sh
--db-seed cbs=./backups/cbs.sql.gz
--db-seed cbs=oci://ghcr.io/acowtancy/legacy-cbs-seed:2026-05-06
--db-seed cbs=oci://ghcr.io/acowtancy/legacy-cbs-seed@sha256:...
```

Local paths stay useful. OCI is a transport and storage wrapper, not a
replacement for SQL payloads.

## Metadata Rules

Every resolved artifact must have metadata shaped like
`effigy.artifact.v1`.

Required fields:

- schema
- kind
- source type
- source ref or path
- digest when available
- staged root
- primary files

OCI artifacts should carry embedded `effigy-artifact.json` metadata when
possible. Effigy may synthesize minimal metadata for local files and legacy OCI
payloads that do not embed metadata yet.

Effigy metadata is descriptive. App logic still decides what a file means.

## Staging Rules

Effigy stages artifacts under a controlled `.effigy/local` path.

The staged path must be deterministic enough for reports and debugging, but it
must not rely on mutable tags alone when a digest is known.

Staging must produce:

- staged artifact root
- metadata file
- primary payload file paths
- source ref/path
- digest when known

Task execution must receive resolved artifact inputs as structured runtime
context. Env vars may exist as compatibility shims, but they are not the
primary contract.

## Seed and Dump Integration

Existing seed/dump command UX should accept artifact sources laterally.

Required integrations:

- `bootstrap --db-seed <target>=<artifact-source>`
- `container data seed --db-seed <target>=<artifact-source>`
- `container data dump --db-dump <target>=<artifact-destination>`

For seed operations, local files and OCI artifacts must resolve to the same
staged artifact shape before app-specific seed logic runs.

For dump operations, local destinations write local SQL payloads. OCI
destinations must stage a dump, package metadata, push the artifact, and report
the immutable digest.

## UAT and Deployment Rules

Artifacts must be usable from UAT.

UAT assumptions:

- Effigy may be installed as an operator tool
- Effigy is not part of normal request serving
- registry auth is provided by the deployment environment
- artifact refs are passed explicitly
- digest-pinned refs are preferred for audited apply operations

Effigy must record apply/capture reports suitable for an operation ledger.

An apply record should include:

- environment label
- artifact ref
- artifact digest
- artifact kind
- staged root
- invoked task or command
- result
- timestamp

The app should own DB-level idempotency and migration history. Effigy owns the
outer artifact operation record.

## Security Rules

Private data artifacts must be safe by default.

Rules:

- never log registry tokens
- do not assume public registry access
- prefer existing OCI credential stores where possible
- report immutable digests after pull/push
- allow digest-pinned inputs
- make push operations explicit
- avoid accidental public publish defaults
- keep local cache/staging paths predictable and inspectable

For Acowtancy, seed artifacts are assumed private unless explicitly stated
otherwise.

## Acowtancy Boundary

Effigy should own for Acowtancy:

- OCI pull/push
- local staging
- artifact metadata
- digest reporting
- seed/dump input normalization
- UAT apply/capture reports
- invocation of app-local Rust binaries with resolved artifact context

Acowtancy should own:

- legacy MySQL import semantics
- migration/coercion into the new data model
- layering newer legacy snapshots with UAT-created content
- data validation
- production cutover decisions

## Drift Triggers

Update this contract when:

- artifact CLI names or ref syntax change
- supported source/destination kinds change
- metadata schema changes
- seed/dump integration changes
- UAT apply/capture semantics change
- artifact operation ledger fields change

## First Proof

The first proof should use Acowtancy as a design target without moving its
migration engine into Effigy:

1. resolve a local SQL seed through the artifact substrate
2. resolve an OCI seed through the same substrate
3. pass both into existing seed execution surfaces
4. prove UAT-shaped apply reporting
5. prove capture planning for a generated SQL dump
