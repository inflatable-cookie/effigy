# State Stack And Layered Seed Guide

This guide explains how to declare, plan, apply, and capture ordered system
state using Effigy's state-stack framework.

## What It Is

A **state stack** is an ordered list of layers that builds or rebuilds an
environment's data and schema. Each layer has a role (what it means in the
lifecycle) and an apply mode (how it executes).

Use state stacks when you need:

- reproducible database schema + seed data for new developers
- UAT environments built from migrated legacy baselines
- captured changes that can replay on top of refreshed imports
- audit trails of what exactly built an environment

## Config

Declare stacks in `effigy.toml` under `[state.<name>]`:

```toml
[state.uat]
schema = "effigy.state-stack.v1"
name = "acme-uat"
environment = "uat"

[[state.uat.layers]]
key = "structure"
role = "structure"
source = "db:migrate"
apply_mode = "task"
environment_policy = "all"

[[state.uat.layers]]
key = "baseline"
role = "baseline-seed"
source = "db:seed"
apply_mode = "task"
environment_policy = "all"

[[state.uat.layers]]
key = "legacy"
role = "legacy-import"
source = "oci://ghcr.io/acme/legacy-snapshot:v1.2.3"
apply_mode = "artifact"
environment_policy = "non-production"
```

### Layer fields

| Field | Required | Description |
|---|---|---|
| `key` | yes | unique layer identifier |
| `role` | yes | lifecycle position: `structure`, `baseline-seed`, `legacy-import`, `dev-overlay`, `uat-capture`, `full-capture` |
| `source` | yes | repo path, task selector, or `oci://` artifact ref |
| `apply_mode` | yes | `task`, `artifact`, `sql`, `manual`, or `checkpoint` |
| `environment_policy` | yes | `all`, `dev-only`, `non-production`, `production`, or `capture-only` |
| `depends_on` | no | keys that must complete before this layer; each key must name an earlier layer (declaration order is apply order) |
| `artifact_kind` | no | payload type: `sql-dump`, `migrated-base-snapshot`, `content-overlay`, `object-store` |
| `hook` | no | app-owned task selector or inline task definition to run after the layer |

Inline hooks use the same run syntax as `[tasks]` and are useful when the
finalize logic belongs only to one layer:

```toml
[[state.uat.layers]]
key = "legacy-media"
role = "media-library"
source = "oci://ghcr.io/acme/media:uat"
apply_mode = "artifact"
environment_policy = "all"
artifact_kind = "object-store"
hook = [{ rhai = "state/apply-media.rhai" }]
```

For a single step, hooks can use the compact inline task shape, including
task-level `run_in`:

```toml
hook = { rhai = "state/apply-media.rhai", run_in = "host" }
```

### Capture profiles

Declare capture presets so operators do not need to remember flags:

```toml
[state.uat.captures.new-content]
role = "uat-capture"
source_env = "uat"
source = ".effigy/state-captures/captures/{key}.tar"
ref = "oci://ghcr.io/acme/state:{key}"
task = "state:capture-new-content"
```

Capture `task` may also be declared inline with normal task run syntax when the
logic exists only for that capture profile:

```toml
[state.uat.captures.media]
role = "full-capture"
source_env = "legacy"
source = ".effigy/state-captures/captures/{key}/media"
ref = "oci://ghcr.io/acme/media:{key}"
task = [{ rhai = "state/capture-media.rhai" }]
```

Single-step capture tasks can also use compact inline task syntax:

```toml
task = { rhai = "state/capture-media.rhai", run_in = "host" }
```

## Commands

### Plan (read-only)

```sh
# Plan the default stack declared in the composed manifest
effigy state plan

# Plan a specific stack
effigy state plan uat

# Plan from a standalone manifest file
effigy state plan --manifest state/production.toml

# Write the plan report for later inspection
effigy state plan --write-report
```

Plan is always safe. It resolves layers, checks policies, and reports what
would happen without mutating anything.

### Apply (mutating)

```sh
# Preview apply without executing
effigy state apply uat

# Execute the stack
effigy state apply uat --yes

# Execute while skipping a prerequisite layer already run by a wrapper task
effigy state apply uat --yes --skip-layer structure
```

Apply runs layers in order. Failures stop later layers. Reports are written to
`.effigy/reports/state/<stack>/`. Skipped layers remain visible in the apply
report with `status = "skipped"` so wrapper workflows can avoid duplicate work
without hiding stack lineage.

On a terminal, each executed layer is a live step that names the layer and the
action now running (`running task layer`, `importing SQL layer`, `staging
artifact layer`, then `running apply hook` if the layer declares a hook). The
step ends with one ✓ or ✗ for that layer. `--json` and piped output draw
nothing. Real SQL-layer spinner timing still needs an exact-candidate proof;
the SQL label is the same step path as task and artifact layers.

### Capture

```sh
# Preview capture using a named profile
effigy state capture uat new-content

# Execute and stage local artifacts
effigy state capture uat new-content --yes

# Also push to OCI
effigy state capture uat new-content --yes --push
```

Capture produces replayable layers from a running environment. It is two-phase:
local staging first, then explicit `--push` to publish. On a terminal the
running capture task and the artifact stage share one live step that names the
produced layer, then one ✓ or ✗. `--json` and piped output draw nothing.

### History

```sh
# Latest reports for a stack
effigy state history uat

# Filter by kind
effigy state history uat --kind capture --limit 5

# Drill into one lineage
effigy state history uat --lineage <ID>
```

## Layer Roles

| Role | Typical Use |
|---|---|
| `structure` | schema migrations, table creation |
| `baseline-seed` | lookup rows, roles, invariants |
| `legacy-import` | migrated data from an external system |
| `dev-overlay` | local-only fixtures |
| `uat-capture` | changes authored during UAT that should replay later |
| `full-capture` | complete environment snapshot for cloning elsewhere |

## Apply Modes

| Mode | Behaviour |
|---|---|
| `task` | run a repo task (e.g. `db:migrate`) |
| `artifact` | stage and apply an OCI or local artifact |
| `sql` | execute SQL through the configured database target |
| `manual` | plan includes it; operator runs it out-of-band |
| `checkpoint` | plan includes it; no action, just a label |

## Safety Defaults

- `state apply` is plan-only unless `--yes` is supplied.
- `state capture` is plan-only unless `--yes` is supplied.
- Layer `environment_policy` blocks production-ineligible layers from production
  environments.
- OCI artifact refs under `digest-pinned` policy block mutable tags.

## Report Layout

State reports are plain files under `.effigy/reports/state/`:

```text
.effigy/reports/state/<stack>/
  latest-plan.json
  latest-apply.json
  latest-capture.json
  latest-capture-set.json
  history/
    <timestamp>-plan-<lineage>.json
    <timestamp>-apply-<lineage>.json
    <timestamp>-capture-<lineage>.json
    <timestamp>-capture-set-<key>.json
```

These are operator artifacts, not proof of execution. The canonical proof is the
lineage record inside each report.

## Standalone Data Artifacts

`state artifact` inspects, stages, and captures data payloads on their own,
outside a stack capture. It moved here from the retired top-level
`effigy artifact` command in v0.15, with the same flags and JSON payloads:

```sh
effigy state artifact inspect ./data/legacy.sql.gz --json
effigy state artifact stage oci://ghcr.io/acme/private-data:uat --json
effigy state artifact capture ./dumps/uat.sql.gz \
  --ref oci://ghcr.io/acme/uat-content:2026-05-06 --push --json
```

See [`072-artifact-commands-guide.md`](072-artifact-commands-guide.md) for the
full command guide.

## Common Workflow

1. Define the stack in `effigy.toml`.
2. Run `effigy state plan` to verify layer order and policies.
3. Run `effigy state apply uat --yes` to build the environment.
4. After UAT changes, run `effigy state capture uat new-content --yes --push`.
5. Later, refresh the legacy import layer and re-run `apply` to rebuild.

## JSON Output

All state commands support `--json` for machine-readable reports. See
[`017-json-output-contracts.md`](017-json-output-contracts.md) for schema
ids: `effigy.state-stack.plan.v1`, `effigy.state-stack.apply.v1`,
`effigy.state-stack.capture.v1`.
