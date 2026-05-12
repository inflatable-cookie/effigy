# Deployment Transaction Guide

This guide explains how to deploy Effigy-managed applications to UAT and
production environments using the deployment transaction system.

## What It Is

The deployment transaction system takes a repo from a local operator checkout to
a provider deployment in a predictable, repeatable way. It answers:

- which code ref is being deployed
- which state stack built the database baseline
- which OCI artifacts were applied
- which release evidence gates the deploy
- which provider services are targeted
- whether health checks passed

It is provider-neutral at the plan level. Railway and Render adapters ship with
Effigy; provider packages extend support without core changes.

## When To Use It

Use deployment transactions when you need:

- repeatable UAT deploys from `main` or a feature branch
- production deploys gated by release evidence
- history of what was deployed and when
- redeploy of a previous immutable deployment

Do not use it for:

- provider account bootstrap (create projects, services, databases)
- secret value creation
- automatic rollback of database or media state

## Config

Declare environments in `effigy.toml` under `[deploy.<env>]`:

```toml
[deploy.uat]
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acme-uat"
artifact_policy = "digest-preferred"

[deploy.uat.provider]
adapter = "railway"

[deploy.uat.preflight]
require_clean_worktree = false
require_provider_resources = true
require_provider_variables = true
require_domains = true

[deploy.uat.hooks]
before_state = "hooks:pre-deploy"
after_deploy = "hooks:smoke-test"
```

Production example:

```toml
[deploy.production]
state = "production"
code_ref = "release-tag"
release_policy = "required"
provider_project = "acme-production"
artifact_policy = "digest-pinned"

[deploy.production.provider]
adapter = "railway"

[deploy.production.preflight]
require_clean_worktree = true
require_provider_resources = true
require_provider_variables = true
require_domains = true
require_release_gates = true
```

### Config fields

| Field | Required | Description |
|---|---|---|
| `adapter` | yes | provider: `railway`, `render` |
| `state` | no | named state stack to apply before deploy |
| `code_ref` | no | `branch:<name>`, `tag:<name>`, `sha:<hash>`, `release-tag` |
| `release_policy` | no | `none`, `optional`, `required` |
| `provider_project` | no | provider-side project or service identifier |
| `artifact_policy` | no | `mutable-ok`, `digest-preferred`, `digest-pinned` |
| `[preflight]` | no | safety requirements for this environment |
| `[hooks]` | no | repo tasks to run at transaction stages |

## Commands

### Model (read-only)

```sh
# Derive the provider-neutral deployment model
effigy deploy model --json
```

This emits `deploy.model.v1` with services, backing services, domains, secrets,
and warnings. It is the same model used by `deploy export`.

### Plan (read-only)

```sh
# Plan a UAT deploy
effigy deploy plan uat

# Plan and write a durable report
effigy deploy plan uat --write-report

# Machine-readable plan
effigy deploy plan uat --json
```

Plan resolves the full transaction without mutating anything:

1. load manifest and select `[deploy.uat]`
2. derive `deploy.model.v1`
3. resolve the code ref to a commit or tag
4. evaluate release evidence when required
5. resolve and plan the state stack when configured
6. enforce artifact digest policy
7. run provider preflight checks
8. plan hooks and health checks
9. emit `effigy.deploy.plan.v1`

### Apply (mutating)

```sh
# Deploy to UAT
effigy deploy apply uat --yes

# Deploy to production
effigy deploy apply production --yes
```

Apply re-runs planning, then executes:

1. write active deployment record
2. run `before_state` hook
3. apply state stack when configured
4. run `after_state` hook
5. trigger provider deploy from the resolved code ref
6. poll provider deployment status
7. run health checks
8. run `after_deploy` smoke hook
9. write final deployment report
10. clear active deployment record

Failures stop later stages. `--yes` is required.

### Status

```sh
effigy deploy status uat
effigy deploy status uat --json
```

Reports the latest deployment for the environment, or the active one if a
deploy is in flight.

### History

```sh
effigy deploy history uat
effigy deploy history uat --limit 10
effigy deploy history uat --json
```

Lists prior deployments with ids, timestamps, code refs, and outcomes.

### Redeploy

```sh
effigy deploy redeploy uat --deployment <ID> --yes
```

Replays a previous deployment using its recorded immutable inputs. This is
evidence-backed replay, not rollback. Database and media rollback are not
promised.

## Safety Defaults

- `deploy apply` requires `--yes`.
- `release_policy = "required"` blocks production deploys without release
  evidence.
- `artifact_policy = "digest-pinned"` blocks mutable OCI tags.
- `require_clean_worktree = true` blocks deploys with uncommitted changes.
- Provider adapters do not create projects, services, databases, secrets, or
domains. Missing setup surfaces as explicit blockers.

## Provider Adapters

| Provider | Status | Notes |
|---|---|---|
| Railway | available | full plan/apply/status/history |
| Render | available | preflight checks enforced; live mutation gated |

External deploy-provider packages extend support. A provider package provides
`preflight.rhai`, `apply.rhai`, and `status.rhai` scripts. Core Effigy keeps the
transaction frame, report persistence, and safety gates.

## Integration With State Stacks

When `state = "<stack>"` is configured:

- `deploy plan` embeds a state apply summary.
- `deploy apply` calls `effigy state apply` before provider deploy.
- state reports remain canonical for state lineage.
- deploy reports store state report paths and lineage ids.

## Report Layout

Deployment reports live under `.effigy/reports/deploy/`:

```text
.effigy/runtime/deploy/active/<env>.json
effigy/reports/deploy/<env>/latest.json
effigy/reports/deploy/<env>/history/<timestamp>-<deployment-id>.json
```

Schema ids:

- `effigy.deploy.plan.v1`
- `effigy.deploy.apply.v1`
- `effigy.deploy.status.v1`
- `effigy.deploy.history.v1`

## Common Workflow

### UAT loop

```sh
# 1. plan
effigy deploy plan uat

# 2. apply
effigy deploy apply uat --yes

# 3. capture UAT changes through state
effigy state capture uat new-content --yes --push

# 4. verify status
effigy deploy status uat
```

### Production release

```sh
# 1. ensure release is ready
effigy release status --check-gates

# 2. plan production
effigy deploy plan production

# 3. apply production
effigy deploy apply production --yes

# 4. verify
effigy deploy status production
```

## JSON Output

All deploy commands support `--json` for machine-readable reports. See
[`017-json-output-contracts.md`](./017-json-output-contracts.md) for schema
details.
