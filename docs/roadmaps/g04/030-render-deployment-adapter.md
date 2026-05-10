# 030 - Render Deployment Adapter

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-10
Depends on:
- [`029-railway-deployment-adapter.md`](./029-railway-deployment-adapter.md)

## Goal

Add Render support to the same provider-neutral deployment transaction system.

Railway lands first. Render must land before the v0.6.0 deployment closeout.

## Scope

- implement Render provider adapter behind the same trait as Railway
- add Render preflight:
  - execution backend selected and documented
  - auth/session available
  - blueprint or service targets accessible
  - required environment variable names present
  - required databases/resources visible
  - domains verified when configured
- support:
  ```toml
  [deploy.uat]
  provider = "render"
  ```
- run through the same plan/apply/status/report schemas as Railway

## Key Design Decision

Settle the execution backend before implementation opens:

- prefer CLI-backed execution if Render's CLI surface is sufficient
- fall back to API-backed execution only if Render lacks usable CLI deployment,
  status, or preflight operations
- keep the provider adapter boundary identical either way

## Render Rules

- do not create secrets in v0.6.0
- do not silently provision missing resources
- respect the existing Render export contract
- provider-specific gaps become explicit blockers or warnings
- keep Render behavior isolated to the provider adapter

## Non-Goals

- no provider secret creation
- no unbounded Render resource provisioning
- no database rollback
- no provider-specific report schema fork

## Acceptance Criteria

- Render can run through `deploy plan`, preflight, apply, status, and report
  using the same deployment transaction model
- Render-specific behavior is isolated to the adapter
- existing `deploy export render` remains compatible
- Railway and Render share JSON report schemas

## Validation

- mocked Render backend tests
- missing auth/service/env/domain/resource tests
- successful preflight/apply/status tests
- cross-provider schema parity tests
- existing Render export tests remain passing

## Next Task

Add deployment status, history, and evidence-backed redeploy.
