# 636 - Close Deployment Transaction Suite

Lane: [`064-deployment-transaction-system-strict-lane.md`](../064-deployment-transaction-system-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-10
Completed: 2026-05-10

## Goal

Close the v0.6.0 deployment transaction implementation suite after the
deploy-specific command, report, provider, status, history, redeploy, and
Acowtancy documentation slices landed.

## Scope

- add deploy env config parsing from composed `[deploy.<env>]`
- add `deploy plan <env>` with optional report persistence
- add `deploy apply <env> --yes`
- add provider-neutral Railway and Render transaction report support
- add `deploy status <env>`
- add `deploy history <env>`
- add `deploy redeploy <env> --deployment <ID> --yes`
- update deploy help, command reference, JSON contract guidance, and examples
- update the Acowtancy migration problem document with deployment flows
- close `g04.027` through `g04.032`

## Non-Goals

- no provider project creation
- no provider service creation
- no provider secret creation
- no provider domain creation
- no automatic database or media rollback
- no release prepare or execute
- no `.github/workflows/` edits

## Exit Condition

This card is complete when deployment transactions can be planned, applied,
queried, listed, and redeployed through durable reports, and the g04 deployment
suite is marked complete.

## Validation

- focused deploy parser, runner, help, and report tests
- docs path checks
- `git diff --check`

## Closeout

The deployment transaction surface now exists as the v0.6.0 operator frame.
The first slice records provider operation evidence through the shared report
boundary while keeping provider setup creation, secrets, rollback, and
app-specific migration semantics out of Effigy.

## Next Task

Hand off to release readiness. Release execution remains human-owned.
