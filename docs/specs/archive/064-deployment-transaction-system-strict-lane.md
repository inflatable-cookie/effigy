# 064 - Deployment Transaction System Strict Lane

Roadmap: [`g04.027`](../roadmaps/g04/027-deployment-transaction-system.md)

Status: Complete
Owner: Platform
Created: 2026-05-10

## Purpose

Define and implement the v0.6.0 deployment transaction system without
colliding with the active `g04.022` through `g04.026` work.

This lane owns:

- `[deploy.<env>]` environment config
- `effigy deploy plan <env>`
- `effigy deploy apply <env> --yes`
- provider-neutral deployment reports
- Railway provider apply adapter
- Render provider apply adapter
- deployment status/history/redeploy
- Acowtancy deployment proof and closeout

## Hard Boundaries

- keep `deploy export` file-oriented and separate from live deployment
- Railway lands before Render
- Render must land before the v0.6.0 deployment closeout
- provider setup is validated, not created
- provider credentials are operator-owned
- secrets are referenced by name only and never printed
- state-stack reports remain canonical for state lineage
- deploy reports reference state reports instead of duplicating state payloads
- deploy may consume release evidence, but must not run release prepare or
  release execute
- no automatic database or media rollback
- no Acowtancy-specific transform or reconciliation logic in Effigy
- no `.github/workflows/` edits

## Coordination Boundaries

The `g04.022` through `g04.026` work completed before this lane moved from
queued planning into implementation.

## Current Ready Card

None. The lane is complete for the v0.6.0 deployment slice.

## Execution Chain

- `635` complete: queued the deployment strict lane, recorded coordination
  boundaries, and selected the first future implementation boundary
- `636` complete: closed the deployment transaction suite with config parsing,
  plan/apply/status/history/redeploy commands, Railway/Render transaction
  report support, docs, tests, and Acowtancy proof documentation

## Exit Condition

This lane is complete when Effigy can plan, apply, inspect, and replay
provider-neutral UAT and production deployments through Railway and Render,
with state-stack lineage, artifact policy, release evidence, provider reports,
hooks, and health checks captured in durable deployment reports.

The first slice records provider transaction evidence through the shared report
boundary. Provider setup creation, secret creation, provider rollback, and
database/media rollback remain out of scope.

## Next Task

Hand off to release readiness. Release execution remains human-owned.
