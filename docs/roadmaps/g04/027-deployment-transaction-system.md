# 027 - Deployment Transaction System

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-10
Depends on:
- [`026-shared-dispatcher-and-exec-collapse.md`](./026-shared-dispatcher-and-exec-collapse.md)
- [`019-state-stack-and-layered-seed-framework.md`](./019-state-stack-and-layered-seed-framework.md)
- [`018-oci-artifact-closeout-and-proof-matrix.md`](./018-oci-artifact-closeout-and-proof-matrix.md)
- [`../../contracts/019-deployment-transaction-system-contract.md`](../../contracts/019-deployment-transaction-system-contract.md)

## Goal

Define the full deployment transaction model for v0.6.0.

The model covers code source, provider target, state stack, artifact policy,
release policy, hooks, health checks, and durable deployment reports.

## Scope

- promote the deployment system contract
- define provider-neutral transaction stages
- define environment deployment config under `[deploy.<env>]`
- define how deploy composes:
  - `deploy.model.v1`
  - state-stack lineage
  - OCI artifact refs and digests
  - release evidence
  - provider preflight/apply adapters
  - repo hooks
- lock v0.6.0 execution posture:
  - Railway first
  - Render second
  - provider adapter backed
  - no provider resource or secret creation
  - no database rollback

## Non-Goals

- no command implementation
- no provider execution
- no Render or Railway CLI calls
- no Acowtancy-specific migration logic
- no release execution
- no `.github/workflows/` edits

## Why Now

Effigy now has the separate primitives needed for deployment:

- provider-neutral deployment model and export adapters
- OCI artifact transport
- state-stack apply/capture/history
- release readiness and gates
- task execution and Rhai hooks

Acowtancy needs those primitives composed into one predictable UAT and
production deployment transaction.

## Acceptance Criteria

- contract is decision-complete
- the g04 deployment suite is linked from the roadmap README
- existing `deploy export` remains explicitly separate from live deployment
- v0.6.0 provider order is documented: Railway first, Render before closeout

## Validation

- docs path checks for new roadmap and contract links
- `git diff --check`

## Next Task

Continue to `g04.028` for deploy config, plan, and reporting.
