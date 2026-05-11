# 019 - Deployment Transaction System Contract

Status: Active
Owner: Platform
Created: 2026-05-10

## Purpose

Effigy needs a deployment transaction system that can take a repo from a local
operator checkout to a predictable UAT or production deployment without making
each app reinvent orchestration.

The existing deployment surface exports provider files. That remains useful,
but it does not answer the larger operational question:

- which code ref is being deployed
- which state stack built the database and media baseline
- which OCI artifacts were applied
- which release evidence gates production
- which provider project and services are targeted
- which app-owned hooks ran
- which health checks passed
- which immutable inputs can be redeployed later

This contract defines the app-agnostic transaction layer above the existing
deploy model, state-stack framework, artifact substrate, release system, and
provider adapters.

## Scope

The deployment transaction system owns:

- environment deployment config under `[deploy.<env>]`
- provider-neutral deploy planning
- code-source resolution
- release-policy evaluation
- state-stack orchestration as one deploy stage
- artifact digest-policy checks
- provider preflight and apply adapter boundaries
- repo-owned hook invocation
- health/smoke check orchestration
- active/latest/history deployment reports
- evidence-backed redeploy of previous immutable inputs

The deployment transaction system does not own:

- app-specific data transforms
- schema-level conflict resolution
- media rewrite semantics
- provider account bootstrap
- provider secret value creation
- automatic database or media rollback
- release prepare or release execute
- post-go-live legacy sync

## Existing Surface Boundary

`effigy deploy model --json` remains the provider-neutral shape derivation.

`effigy deploy export render|railway` remains static file generation.

The new transaction surface is separate:

```sh
effigy deploy plan <env>
effigy deploy apply <env> --yes
effigy deploy status <env>
effigy deploy history <env>
effigy deploy redeploy <env> --deployment <ID> --yes
```

File export must not gain hidden live deployment behavior.

## Environment Config

Deployment config is normal composed Effigy manifest config.

Minimum shape:

```toml
[deploy.uat]
provider = "railway"
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "acowtancy-uat"
artifact_policy = "digest-preferred"
```

Production shape:

```toml
[deploy.production]
provider = "railway"
state = "production"
code_ref = "release-tag"
release_policy = "required"
provider_project = "acowtancy-production"
artifact_policy = "digest-pinned"

[deploy.production.preflight]
require_clean_worktree = true
require_provider_resources = true
require_provider_variables = true
require_domains = true
require_release_gates = true
```

Supported first-round fields:

- `provider`
  - required
  - `railway` first, `render` before the v0.6.0 closeout
- `state`
  - optional named state stack
- `code_ref`
  - `branch:<name>`, `tag:<name>`, `sha:<hash>`, or `release-tag`
- `release_policy`
  - `none`, `optional`, or `required`
- `provider_project`
  - provider-side project identity
- `artifact_policy`
  - `mutable-ok`, `digest-preferred`, or `digest-pinned`
- `[deploy.<env>.preflight]`
  - environment-specific safety requirements
- `[deploy.<env>.hooks]`
  - repo task selectors for transaction stages

## Transaction Stages

`deploy plan` resolves the transaction without mutating provider or state:

1. load the composed manifest
2. select `[deploy.<env>]`
3. derive `deploy.model.v1`
4. resolve the code ref
5. resolve release evidence when required
6. resolve the state stack when configured
7. plan state apply
8. enforce artifact digest policy
9. build provider preflight/apply plan
10. run provider read-only preflight checks when available
11. plan hooks and health checks
12. emit `effigy.deploy.plan.v1`

`deploy apply --yes` re-runs planning and executes:

1. write active deployment record
2. run `before_state` hook when configured
3. run state apply when configured
4. run `after_state` hook when configured
5. trigger provider deploy from the resolved code ref
6. poll provider deployment status
7. run health checks
8. run `after_deploy` smoke hook when configured
9. write final deployment report
10. clear active deployment record

Failures stop later stages.

## Release Policy

Deploy consumes release evidence. It must not run release prepare or release
execute.

Policies:

- `none`
  - no release checks
- `optional`
  - include release status when available; warnings only
- `required`
  - require release-ready evidence and configured gates

Production defaults should require:

- clean worktree
- release tag or release evidence
- release gates
- digest-pinned state artifacts

UAT defaults may allow:

- branch refs
- dirty worktree only when configured
- digest-preferred artifacts
- no release gates unless configured

## State And Artifact Rules

State remains its own command family.

Deploy may invoke state as a transaction stage.

Rules:

- `deploy plan` embeds a state apply summary, not full state semantics.
- `deploy apply` calls the same state apply path as `effigy state apply`.
- state reports remain canonical for state lineage.
- deploy reports store state report paths and lineage ids.
- artifact digest policy is enforced before state apply.
- `digest-pinned` blocks mutable tag-only OCI refs.
- app-specific artifact payload semantics remain app-owned.

## Provider Adapter Boundary

Provider execution must sit behind an adapter boundary:

```rust
trait DeployProviderAdapter {
    fn provider_name(&self) -> &'static str;
    fn preflight(&self, request: DeployProviderPreflightRequest)
        -> DeployProviderPreflightReport;
    fn apply(&self, request: DeployProviderApplyRequest)
        -> DeployProviderApplyReport;
    fn status(&self, request: DeployProviderStatusRequest)
        -> DeployProviderStatusReport;
}
```

Railway is the first apply adapter.

Render uses the same transaction boundary. In the v0.6.0 planning slice it
must report explicit preflight checks for the adapter, required variable names,
and domains. Live Render mutation remains gated behind existing provider
credentials and already-created services/resources.

Provider credentials are operator-owned.

Provider adapters must not:

- create projects in v0.6.0
- create services in v0.6.0
- create provider databases/resources in v0.6.0
- create secrets or variables in v0.6.0
- create domains in v0.6.0
- silently choose fallback projects or services
- print secret values

Missing provider setup should become explicit blockers with remediation.

## Reports

Report layout:

```text
.effigy/runtime/deploy/active/<env>.json
.effigy/reports/deploy/<env>/latest.json
.effigy/reports/deploy/<env>/history/<timestamp>-<deployment-id>.json
```

Schema ids:

- `effigy.deploy.plan.v1`
- `effigy.deploy.apply.v1`
- `effigy.deploy.status.v1`
- `effigy.deploy.history.v1`

Minimum plan fields:

- `env`
- `provider`
- app identity
- deploy model summary
- code ref and resolved commit/tag
- release evidence
- state stack and lineage summary
- artifact refs and digests
- provider preflight results
- hooks
- health checks
- warnings
- blockers

Minimum apply fields:

- deployment id
- env
- provider
- started and finished timestamps
- resolved code ref
- release evidence
- state apply report path and lineage id
- provider operation report
- hook results
- health/smoke results
- final status
- redeploy input summary

## Redeploy Rules

`deploy redeploy` is evidence-backed replay, not rollback.

Rules:

- redeploy uses recorded immutable inputs
- mutable branch refs are redeployable only when the resolved commit is recorded
  and still available
- mutable OCI tags are not redeployable under `digest-pinned`
- database and media rollback are not promised
- provider rollback shortcuts may be added later only when they preserve the
  same report and reproducibility rules

## Acowtancy Proof

The proof target is the Acowtancy UAT/rebase deployment loop:

```sh
effigy deploy plan uat
effigy deploy apply uat --yes
effigy state capture uat new-content --yes --push
effigy deploy plan production
effigy deploy apply production --yes
```

Effigy owns the transaction frame.

Acowtancy still owns:

- legacy snapshot transforms
- content conflict resolution
- media binding and rewrite semantics
- post-go-live legacy sync

## Validation Expectations

Implementation lanes should include:

- CLI parser tests
- manifest parsing tests
- JSON contract tests
- report persistence tests
- git ref resolution tests
- release policy tests
- state-stack integration tests
- artifact digest policy tests
- provider adapter tests with mocked provider output
- secret redaction tests
- failure-stage stop tests
- Railway and Render schema parity tests

## Next Task

Use this contract as the anchor for `g04.027` through `g04.032`. Implement
Railway first, then Render, then status/history/redeploy and the Acowtancy
proof closeout.
