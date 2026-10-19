# Deployment Guide

Effigy v0.15 supports provider-neutral model inspection and provider file export.
`deploy apply` and `deploy redeploy` are **not yet supported**. They exit nonzero,
including with `--yes`, before resolving provider packages or running scripts.
No deployment, rollback, active record or success report is produced.

The full execution design lives in Queue's `deploy` lane. The settled v0.15
boundary and remaining design question are recorded in
[Q-003](../knowledge/questions.md#q-003--production-deployment-ownership).
The [transaction contract](../knowledge/contracts/019-deployment-transaction-system-contract.md)
owns this boundary; its later sections describe the target design.

## Model and export

```sh
# Inspect the provider-neutral model
effigy deploy model --json

# Preview provider file generation
effigy deploy export render --path infra/render --plan

# Generate provider files
effigy deploy export render --path infra/render
```

Model emits `deploy.model.v1` with services, backing services, domains, secrets
and warnings, derived from the effective manifest and bundle. Export uses that
model and a configured external provider package. It generates files; it does
not deploy them. Preview behavior belongs to the provider's export capability.

```toml
[deploy.providers.render]
source = { type = "git", url = "git@github.com:inflatable-cookie/effigy-provider-render.git", ref = "main" }
```

The external Render package supplies `render.yaml`. Export requires a package
configured under `[deploy.providers.<provider>]`; provider-specific behavior
stays outside core Effigy.

## Environment configuration

Existing `[deploy.<env>]` configuration remains readable, including Acowtancy's
`infra/deploy.toml`. No manifest migration is required for model/export.
Environment declarations describe intended work; they do not enable execution.

```toml
[deploy.production]
state = "production"
code_ref = "release-tag"
release_policy = "required"
provider_project = "example-production"
artifact_policy = "digest-pinned"

[deploy.production.provider]
adapter = "render"

[deploy.production.preflight]
require_release_gates = true

[deploy.production.hooks]
before_state = "hooks:pre-deploy"
after_state = "hooks:post-state"
after_deploy = "hooks:smoke-test"
```

`preflight.require_release_gates` is the supported core preflight option.
Provider resource checks belong to the external package.

## Plan

```sh
effigy deploy plan uat --json
effigy deploy plan uat --write-report
```

Plan inspects environment configuration, resolves code references and provider
packages, runs provider preflight, and lists intended state, hooks and health
checks. It emits `effigy.deploy.plan.v1`. It does not execute those obligations,
verify release gates or enforce immutable artifact pins. A plan with no blockers
is not evidence that a deployment can execute. Provider resolution can acquire
packages and preflight runs provider code. `--write-report` persists the plan.

## Unsupported execution

```sh
# Both refuse in v0.15, with or without --yes
effigy deploy apply uat --yes
effigy deploy redeploy uat --deployment <ID> --yes
```

The refusal names all unimplemented obligations: state and migrations;
`before_state`, `after_state` and `after_deploy` hooks; health checks; release
gates; digest pins; and provider deployment/rollback. Neither a provider success
response nor a historical success report can make these commands succeed.
Removing optional obligations from configuration does not enable execution.
Use model/export to inspect or generate provider configuration while the full
kernel-backed execution design remains in the deploy lane.

## Status, history and existing reports

```sh
effigy deploy status uat --json
effigy deploy history uat --limit 10 --json
```

Status and history can still read v0.14.1 deployment records. Status may invoke
the configured provider's status capability. Old `succeeded` values describe
what the old version recorded; they do not prove state, hooks, health or gates
ran. The unsupported commands leave these files untouched:

```text
.effigy/runtime/deploy/active/<env>.json
.effigy/reports/deploy/<env>/latest.json
.effigy/reports/deploy/<env>/history/<deployment-id>.json
```

## JSON output

The existing `effigy.command.v1` envelope has `ok: false` and a nonzero process
exit for both unsupported commands. `error.details` uses
`effigy.deploy.apply.v1` (schema version 1) with `ok: false`, `status: "blocked"`,
`reason: "not_supported"`, `operation` (`apply` or `redeploy`), `env`, `message`
and `blockers`. There is no deployment id or execution receipt because no run
started. The reason is stable across environments, output modes and `--yes`.

Other report schemas remain `deploy.model.v1`, `effigy.deploy.export.v1`,
`effigy.deploy.plan.v1`, `effigy.deploy.status.v1` and `effigy.deploy.history.v1`.
See [JSON output contracts](017-json-output-contracts.md).
