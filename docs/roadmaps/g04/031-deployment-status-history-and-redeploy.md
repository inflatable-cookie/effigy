# 031 - Deployment Status, History, And Redeploy

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-10
Depends on:
- [`030-render-deployment-adapter.md`](./030-render-deployment-adapter.md)

## Goal

Make deployment operations auditable and replayable.

## Scope

Add:

```sh
effigy deploy status <env> [--json]
effigy deploy history <env> [--json] [--limit <N>]
effigy deploy redeploy <env> --deployment <ID> --yes [--json]
```

Persist:

```text
.effigy/runtime/deploy/active/<env>.json
.effigy/reports/deploy/<env>/latest.json
.effigy/reports/deploy/<env>/history/<timestamp>-<deployment-id>.json
```

## Redeploy Rules

- redeploy uses recorded immutable inputs
- mutable branch refs are not redeployable unless the resolved commit is
  recorded and still available
- mutable OCI tags are not redeployable under `digest-pinned`
- redeploy does not rollback database or media state
- redeploy can replay the same deployment transaction when inputs remain
  reproducible

## Non-Goals

- no automatic provider rollback shortcut
- no database rollback
- no report retention policy
- no cross-machine deployment inventory

## Acceptance Criteria

- status merges active and latest report truth
- history lists deployment reports by env
- redeploy blocks non-reproducible inputs
- redeploy succeeds through mocked providers with recorded immutable refs
- Railway and Render share the same status/history/redeploy schemas

## Validation

- history scan tests
- active/latest status merge tests
- redeploy plan tests
- non-reproducible mutable artifact blocker tests
- mocked provider redeploy tests

## Next Task

Prove the deployment system against Acowtancy's UAT and production deployment
loop, then close the v0.6.0 suite.
