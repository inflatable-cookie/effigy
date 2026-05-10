# 064 - Deployment Transaction System Strict Lane

Roadmap: [`g04.027`](../roadmaps/g04/027-deployment-transaction-system.md)

Status: Queued
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

The `g04.022` through `g04.026` thread owns remote bundle sources, docs-command
cleanup, command-reference completion, container-command decomposition, and
shared dispatcher/exec collapse.

This lane should avoid:

- shared dispatcher refactors until `g04.026` lands
- broad command-reference churn while `g04.024` is active
- container-command or runtime execution rewrites
- bundle-source internals until `g04.022` lands

Safe first implementation areas after activation:

- deploy-specific parser additions
- deploy-specific manifest config types
- deploy-specific report structs
- deploy-specific provider adapter trait
- mocked provider tests
- deployment docs that do not rewrite shared indexes

## Current Ready Card

None. The lane is queued until the currently active task-status query lane and
the `g04.022` through `g04.026` coordination point are clear or deliberately
paused.

## Execution Chain

- `635` complete: queued the deployment strict lane, recorded coordination
  boundaries, and selected the first future implementation boundary

## Future Card Order

- `636`: promote deploy env config and plan report field contract
- `637`: add deploy env config parser
- `638`: add deploy plan command surface
- `639`: add deploy plan report history
- `640`: add provider adapter trait
- `641`: add Railway preflight adapter
- `642`: add Railway apply transaction
- `643`: settle Render execution backend
- `644`: add Render preflight adapter
- `645`: add Render apply transaction
- `646`: add deploy status/history
- `647`: add deploy redeploy
- `648`: close Acowtancy deployment proof
- `649`: close v0.6.0 deployment suite

## Exit Condition

This lane is complete when Effigy can plan, apply, inspect, and replay
provider-neutral UAT and production deployments through Railway and Render,
with state-stack lineage, artifact policy, release evidence, provider reports,
hooks, and health checks captured in durable deployment reports.

## Next Task

Wait for the active coordination point to clear, then execute future card `636`
as the first deploy-specific implementation boundary.
