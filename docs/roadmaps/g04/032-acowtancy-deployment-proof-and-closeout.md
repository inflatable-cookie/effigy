# 032 - Acowtancy Deployment Proof And Closeout

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-10
Depends on:
- [`031-deployment-status-history-and-redeploy.md`](./031-deployment-status-history-and-redeploy.md)

## Goal

Prove the deployment system against the original Acowtancy migration and UAT
problem, then close the v0.6.0 deployment suite.

## Scope

- update Acowtancy-facing documentation
- model Acowtancy UAT deploy config
- model Acowtancy production deploy config
- validate the intended loop:
  ```sh
  effigy deploy plan uat
  effigy deploy apply uat --yes
  effigy state capture uat new-content --yes --push
  effigy deploy plan production
  effigy deploy apply production --yes
  ```
- document how the UAT capture/rebase loop sits beside deployment
- confirm no Acowtancy-specific transform or reconciliation logic entered
  Effigy
- represent both Railway and Render support in the closeout evidence

## Non-Goals

- no post-go-live legacy sync engine
- no Acowtancy transform implementation inside Effigy
- no media rewrite policy inside Effigy
- no database rollback
- no release execution

## Acceptance Criteria

- the original Acowtancy problem document references the deployment transaction
  layer
- UAT and production operator flows are documented
- Railway and Render provider support are both represented in the v0.6.0
  closeout
- remaining post-go-live sync work is explicitly deferred
- v0.6.0 deployment surface is release-ready

## Validation

- docs checks
- `effigy deploy plan uat --json` against Acowtancy config or a faithful
  fixture
- UAT branch/digest-preferred proof
- production release-evidence/digest-pinned proof
- capture/rebase remains state-owned
- `git diff --check`

## Next Task

Hand off to release readiness. Release execution remains human-owned.
