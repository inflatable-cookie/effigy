# Deployment data workflows

Owner: Platform
Created: 2026-11-19 (v0.16 retirement, effigy#1057)

Effigy no longer runs a layered state stack. `effigy state plan|apply|capture|
capture-set|history`, the `[state.*]` manifest table, phase and layer policy,
capture profiles, lineage and history reports, state hooks and state report
files were removed in v0.16 (Tom, 2026-11-19: "the new Acowtancy site isn't
deployed, so it doesn't block removal; lay a path for replacing it, or decide
it isn't needed").

A repository now builds deployment data with its own tasks. Effigy keeps the two
pieces other commands already consume:

- **Local database seed and dump**: `container data seed` and
  `container data dump`, including `oci://` inputs and outputs and explicit
  database target selection.
- **Plain artifact staging**: `effigy state artifact inspect|stage|capture`
  (`effigy.artifact.*.v1` payloads, contract 014) stages a local file or
  directory or an explicit `oci://` ref and reports the immutable digest of what
  it staged. `state artifact capture --ref oci://… --push` is the only verb that
  publishes.

Those two cover transport and target selection. What the data *means*, in what
order it applies, and how a refresh reconciles with captured new-system changes,
is application knowledge and belongs in the repository's tasks.

## What the layered workflow was for

Contract 016 (retired) modelled the Example App / Acowtancy legacy-to-UAT loop:
schema baseline, repo-committed seed data, imported legacy snapshots, optional
dev overlays, then a UAT capture of changes authored in the new system, and a
later rebase that replayed those captures over a refreshed legacy import. It
shipped phase taxonomy, ordered layers, capture profiles, per-layer hooks,
lineage records and `latest`/history report files.

## Is a replacement needed?

No feature is needed. The Acowtancy new site has never been deployed, so no UAT
layered rebuild exists to replace, and nothing in the product has another
consumer of the layering. The one live use of the retired surface was the legacy
Acowtancy site's `snapshot` task, which used `state capture-set` only to run two
staging scripts and push their output. That is already a plain task sequence.

Repositories that used `state apply` should rebuild those environments once with
their ordinary schema/seed/dump tasks and then delete the stack. Data that the
retired surface already wrote (`.effigy/reports/state/`, `.effigy/state-captures/`,
staged artifacts under `.effigy/local/artifacts/`) is left on disk untouched;
nothing reads it now.

## The simple form

Declare ordinary tasks. Each step is a command the repository already owns, so
the order and the transforms stay reviewable in one file:

```toml
# effigy.toml
[tasks."db:setup"]
# Schema then repo-committed seed, both via the built-in data path.
run = [
  "effigy container data seed --file db/schema.sql --target app",
  "effigy container data seed --file db/seed.sql --target app",
]

[tasks."db:legacy-import"]
# A legacy snapshot is one explicit, digest-checked artifact, then a seed.
run = [
  "effigy state artifact stage oci://ghcr.io/acme/legacy-db:2026-05-01 --json",
  "effigy container data seed --file .effigy/local/artifacts/legacy-db/legacy-db.sql --target app",
]

[tasks."db:snapshot"]
# Capture the other direction: dump locally, publish the immutable result.
run = [
  "effigy container data dump --target app --file data/exports/app-{args}.sql",
  "effigy state artifact capture data/exports/app-{args}.sql --ref oci://ghcr.io/acme/app-content:{args} --kind sql-dump --environment uat --push",
]
```

Rules of thumb:

- One task per phase a person or a wrapper actually runs (`db:setup`,
  `db:legacy-import`, `db:snapshot`); a wrapper composes them with `task` steps.
- Pass `oci://` refs explicitly. Effigy does not infer a registry from a
  Docker-like string, and artifact refs, not tags alone, are what carry the
  digest in a report.
- Publish only with `--push` on an explicit `--ref`. Staging stays local.
- Environment eligibility (for example "never run this on production") is a
  task-level decision in the repository, not an Effigy phase policy.
- The application owns merges, transforms and rebase decisions. A "rebase" is
  the repository's own task that composes a fresh import with its captured
  data.

## Legacy Acowtancy `effigy.toml`

`~/Dev/legacy/sites/acowtancy/effigy.toml` still declares:

```toml
[state.legacy-source]
schema = "effigy.state-stack.v1"
name = "acowtancy-legacy-source"
environment = "dev"

[[state.legacy-source.layers]]
key = "legacy-source-root"
role = "baseline-seed"
source = "legacy-source-root"
apply_mode = "checkpoint"
environment_policy = "all"
snapshot_identity = "legacy-source-root@local"

[state.legacy-source.captures.db]
role = "full-capture"
source_env = "dev"
source = "data/exports/source-snapshots/{key}/legacy-db-snapshot.sql"
ref = "oci://ghcr.io/acowtancy/legacy-source-db:{key}"
task = [{ rhai = "infra/source-exports/stage-db-export.rhai" }]

[state.legacy-source.captures.media]  # the same shape for the media directory

[tasks]
snapshot = "effigy state capture-set legacy-source db media --yes --key {args}"
```

That repository is not edited by this change. After upgrading, `[state.*]`
refuses to load and `effigy state capture-set` stops with the retirement error.
The `snapshot` task should become two explicit artifact captures, with the
existing staging scripts as their own steps, and the `[state.legacy-source]`
table should be deleted:

```toml
[tasks."snapshot:db"]
run = [
  { rhai = "infra/source-exports/stage-db-export.rhai" },
  "effigy state artifact capture data/exports/source-snapshots/{args}/legacy-db-snapshot.sql --ref oci://ghcr.io/acowtancy/legacy-source-db:{args} --kind sql-dump --push",
]

[tasks."snapshot:media"]
run = [
  { rhai = "infra/source-exports/stage-media-export.rhai" },
  "effigy state artifact capture data/exports/source-snapshots/{args}/media --ref oci://ghcr.io/acowtancy/legacy-source-media:{args} --kind object-store --push",
]

[tasks.snapshot]
run = ["effigy snapshot:db {args}", "effigy snapshot:media {args}"]
```

The stack's single `checkpoint` layer had no apply behaviour, so nothing else
needs translating.

## Where the facts live

- Artifact transport, metadata and digests: [contract 014](contracts/014-artifact-substrate-contract.md).
- Database target selection and destructive data operations: [container runtime contract](contracts/005-container-runtime-contract.md) and [artifact substrate contract](contracts/014-artifact-substrate-contract.md).
- Container data commands: [guide 063](../guides/063-container-system-guide.md) and [guide 072](../guides/072-artifact-commands-guide.md).
- Deploy keeps `[deploy.<env>].state` only as its environment state label; it
  never applies data: [contract 019](contracts/019-deployment-transaction-system-contract.md).
