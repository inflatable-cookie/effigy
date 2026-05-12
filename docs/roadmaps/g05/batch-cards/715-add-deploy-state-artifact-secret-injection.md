# 715 - Add Deploy State Artifact Secret Injection

Roadmap: [`../004-task-rhai-and-deploy-secret-injection.md`](../004-task-rhai-and-deploy-secret-injection.md)
Strict lane: [`../../../specs/079-task-rhai-deploy-secret-injection-strict-lane.md`](../../../specs/079-task-rhai-deploy-secret-injection-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Make declared secrets available to deploy provider packages, state workflows,
and artifact workflows through scoped runtime context.

## Scope

- pass deploy-targeted secrets into provider package execution context
- pass state-targeted secrets into state workflow execution where needed
- pass artifact-targeted secrets into artifact push/pull/capture workflows
- block missing required secrets before mutating provider/state/artifact work
- redact provider reports and workflow reports by construction

## Non-Goals

- no provider secret creation
- no container startup injection
- no `.env` export
- no undeclared-key reads

## Acceptance

- provider packages can consume declared deploy secrets
- state apply hook tasks can consume declared state secrets
- Rhai execution can opt into deploy, state, and artifact secret target scopes
- missing required secrets block before side effects
- reports can name keys but never include values

## Completed

- Added target-scoped Rhai secret execution so internal callers can opt into
  `deploy`, `state`, and `artifacts` secret access without changing the default
  `rhai` scope.
- Ran deploy provider package scripts with `deploy` secret scope.
- Added execution-request secret targets and used them to inject declared
  `state` secrets into state apply hook task environments.
- Preserved default task secret injection for `targets = ["tasks"]`.
- Added tests for deploy-target Rhai access and state apply hook state-secret
  injection.

## Notes

Artifact-targeted secret access is available to internal Rhai workflow callers
through the same target-scoped execution API. The current built-in artifact
stage/capture commands do not yet have a script execution point, so no
artifact-specific caller was added in this card.

## Validation

- deploy provider fixture tests
- state/artifact fixture tests
- missing-secret blocker tests
- redaction tests
- `cargo check --all-targets`
- `git diff --check`

## Next Task

Execute `716` to close `g05.004`.
