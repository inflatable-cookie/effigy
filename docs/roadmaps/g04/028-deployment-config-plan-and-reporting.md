# 028 - Deployment Config, Plan, And Reporting

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-10
Depends on:
- [`027-deployment-transaction-system.md`](./027-deployment-transaction-system.md)

## Goal

Implement provider-neutral deploy environment config and the first
`effigy deploy plan <env>` surface.

## Scope

- parse `[deploy.<env>]` from the composed manifest
- add:
  ```sh
  effigy deploy plan <env> [--json] [--write-report]
  ```
- derive existing `deploy.model.v1`
- resolve git code refs
- resolve release policy evidence
- resolve configured state stack plans
- enforce artifact digest policy at plan time
- emit `effigy.deploy.plan.v1`
- persist plan reports under `.effigy/reports/deploy/<env>/`

## Public Config

```toml
[deploy.uat]
provider = "railway"
state = "uat"
code_ref = "branch:main"
release_policy = "optional"
provider_project = "example-app-uat"
artifact_policy = "digest-preferred"

[deploy.production]
provider = "railway"
state = "production"
code_ref = "release-tag"
release_policy = "required"
provider_project = "example-app-production"
artifact_policy = "digest-pinned"
```

## Non-Goals

- no provider mutation
- no `deploy apply`
- no Railway or Render execution
- no status/history/redeploy
- no release prepare or execute

## Acceptance Criteria

- `deploy plan <env>` works without provider mutation
- missing deploy env errors clearly
- multiple deploy envs are selectable by name
- plan JSON includes blockers, warnings, state lineage summary, code ref,
  provider, and release policy
- existing `deploy model` and `deploy export` behavior remains compatible

## Validation

- CLI parser tests
- manifest config tests
- JSON contract tests
- focused runner tests with fixture manifests
- report persistence tests
- git ref resolution tests
- release policy tests
- state-stack fixture integration tests
- artifact digest policy tests

## Next Task

Continue to `g04.029` for Railway deployment transaction support.
