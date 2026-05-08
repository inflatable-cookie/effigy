# 072 - Artifact Commands Guide

Use this guide when you need to move versioned data payloads between local
files, OCI registries, and Effigy-managed seed/dump workflows.

This is the practical command guide for `effigy artifact ...` and the artifact
surfaces inside `effigy container ...` and `effigy bootstrap ...`.

Use:

- this guide for the direct artifact commands and everyday workflows
- [`014-artifact-substrate-contract.md`](../contracts/014-artifact-substrate-contract.md)
  for the formal contract, security rules, and drift triggers
- [`063-container-system-guide.md`](./063-container-system-guide.md) for
generated-compose data lifecycle details
- [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md) for
bootstrap seed behavior

## Start Here

Shortest path:

1. install `oras` and log in to your registry
2. inspect or stage a local SQL dump
3. capture and push to an OCI registry
4. consume the artifact from `data seed` or `bootstrap --db-seed`

## Prerequisites

OCI features require the local `oras` CLI:

```bash
brew install oras
```

Authenticate with your registry:

```bash
oras login ghcr.io
```

Effigy uses the normal registry-client auth store. It does not accept tokens in
artifact refs or env files.

## Artifact Commands

### Inspect

Check what Effigy would resolve before staging:

```sh
effigy artifact inspect ./data/legacy.sql.gz --json
effigy artifact inspect oci://ghcr.io/acme/private-data:uat --json
```

Inspect works for local files and OCI refs. It reports the artifact kind,
staged root, primary files, and metadata.

### Stage

Copy a local file or pull an OCI artifact into the repo-owned artifact cache:

```sh
effigy artifact stage ./data/legacy.sql.gz --json
effigy artifact stage oci://ghcr.io/acme/private-data:uat --json
```

Staging is deterministic: the same source produces the same staged root path
under `.effigy/local/artifacts/`.

### Capture

Package a local payload into a new artifact and optionally push it to a registry:

```sh
# Plan only: stage locally, report the planned destination
effigy artifact capture ./dumps/uat.sql.gz \
  --ref oci://ghcr.io/acme/uat-content:2026-05-06 \
  --environment uat --json

# Push: stage locally, then publish to the registry
effigy artifact capture ./dumps/uat.sql.gz \
  --ref oci://ghcr.io/acme/uat-content:2026-05-06 \
  --environment uat --push --json
```

Capture rules:

- capture always stages a local artifact first
- `--push` is required for live registry writes
- digest-pinned refs are invalid push destinations
- the pushed digest is reported in JSON output
- artifact kinds include `sql-dump`, `legacy-source-snapshot`,
  `migrated-base-snapshot`, `uat-content-snapshot`, `content-overlay`,
  `app-specific`

## Artifact Kinds

Kinds are coarse metadata for human and automation routing:

| Kind | Use When |
|---|---|
| `sql-dump` | Standard SQL dump payload |
| `legacy-source-snapshot` | Raw legacy database snapshot |
| `migrated-base-snapshot` | Post-migration base data |
| `uat-content-snapshot` | UAT-created content snapshot |
| `content-overlay` | Layered content update |
| `app-specific` | Custom payload the app interprets |

Effigy metadata is descriptive. App logic still decides what a file means.

## Seed and Dump Integration

Artifacts integrate with container data lifecycle and bootstrap:

### Container Data Seed

```sh
# Local file
effigy container data seed --db-seed ./latest.sql

# OCI artifact
effigy container data seed --db-seed app=oci://ghcr.io/acme/private-data:uat

# Multiple targets
effigy container data seed --db-seed cbs=./cbs.sql --db-seed cbs-mortcalc=./mortcalc.sql
```

### Container Data Dump with Push

```sh
# Dump to local file
effigy container data dump app=./app.sql

# Dump and stage for OCI
effigy container data dump app=oci://ghcr.io/acme/uat-content:2026-05-07 --json

# Dump, stage, and push
effigy container data dump app=oci://ghcr.io/acme/uat-content:2026-05-07 --push --json
```

### Bootstrap with DB Seed

```sh
effigy bootstrap git@github.com:acme/app.git --db-seed app=oci://ghcr.io/acme/seed:v1.0.0
```

OCI refs must use the explicit `oci://` prefix. Unprefixed registry-looking refs
are rejected.

## JSON Output

All artifact commands support `--json`:

```sh
effigy --json artifact inspect oci://ghcr.io/acme/private-data:uat
effigy --json artifact stage ./data/legacy.sql.gz
effigy --json artifact capture ./dumps/uat.sql.gz --ref oci://ghcr.io/acme/uat-content:2026-05-06 --push
```

Payload schemas:

- `effigy.artifact.inspect.v1`
- `effigy.artifact.stage.v1`
- `effigy.artifact.capture.v1`

See [`026-json-payload-examples.md`](./026-json-payload-examples.md) for
realistic sample payloads.

Current boundary:

- these JSON/text reports are the shipped artifact operation record today
- Effigy does not yet persist a separate durable artifact ledger across runs
- if you need audit history now, capture the command report and staged
  metadata in your own operator workflow

## Metadata

Every staged artifact carries an `effigy-artifact.json` metadata file shaped like
`effigy.artifact.v1`:

- `schema`
- `kind`
- `source_type`
- `source_ref` or `source_path`
- `digest` when available
- `staged_root`
- `primary_files`

## Security

- never log registry tokens
- do not assume public registry access
- prefer existing OCI credential stores
- report immutable digests after pull/push
- allow digest-pinned inputs for audited operations
- make push operations explicit with `--push`

## Related Guides

- [`014-artifact-substrate-contract.md`](../contracts/014-artifact-substrate-contract.md)
- [`063-container-system-guide.md`](./063-container-system-guide.md)
- [`057-bootstrap-repo-bringup.md`](./057-bootstrap-repo-bringup.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)
- [`026-json-payload-examples.md`](./026-json-payload-examples.md)

## Next Step

After staging or capturing your first artifact, use `effigy container data seed`
or `effigy bootstrap --db-seed` to wire it into the repo's standard data
lifecycle.
