# 029 - Railway Deployment Adapter

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-10
Depends on:
- [`028-deployment-config-plan-and-reporting.md`](./028-deployment-config-plan-and-reporting.md)

## Goal

Add the first real provider execution adapter for the deployment transaction
system using Railway.

## Scope

- add the provider adapter trait
- add Railway adapter implementation
- add Railway read-only preflight:
  - CLI exists
  - authenticated session exists
  - project is accessible
  - required services exist
  - required variables exist by name
  - required backing services exist
  - domains exist when configured
- add:
  ```sh
  effigy deploy apply <env> --yes [--json]
  ```
- execute the Railway deployment transaction:
  - re-run plan
  - run state apply
  - run hooks
  - trigger provider deploy from git ref
  - poll provider status
  - run health and smoke checks
  - write deployment report

## Railway Rules

- do not create projects
- do not create services
- do not create Postgres or other resources
- do not create variables or secrets
- do not create domains
- block with remediation when setup is missing
- never print secret values

## Non-Goals

- no Render apply adapter
- no provider resource bootstrap
- no provider API client unless Railway CLI cannot cover a required operation
- no database rollback
- no release execution

## Acceptance Criteria

- Railway UAT deploy transaction can be fully planned and applied against a
  mocked adapter
- failures stop later stages
- provider output is captured in `effigy.deploy.apply.v1`
- missing provider setup produces explicit blockers and remediation
- existing `deploy export railway` remains compatible

## Validation

- fake Railway CLI tests
- missing CLI/auth/project/service/resource/variable/domain tests
- successful preflight/apply/status tests
- secret redaction tests
- failed-stage stop tests
- JSON report contract tests

## Next Task

Settle the Render execution backend and implement the Render adapter behind the
same provider boundary.
