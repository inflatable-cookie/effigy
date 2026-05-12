# 714 - Add Rhai Secret API

Roadmap: [`../004-task-rhai-and-deploy-secret-injection.md`](../004-task-rhai-and-deploy-secret-injection.md)
Strict lane: [`../../../specs/079-task-rhai-deploy-secret-injection-strict-lane.md`](../../../specs/079-task-rhai-deploy-secret-injection-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Expose a small declaration-bound secret API to Rhai scripts.

## Scope

- add `effigy.secret(name)` or equivalent module-native helper
- add `effigy.has_secret(name)` or equivalent module-native helper
- enforce `targets = ["rhai"]`
- block undeclared reads
- redact values from Rhai host logs, reports, and errors
- add fixture scripts for present, missing, and undeclared keys

## Non-Goals

- no task injection changes
- no deploy/provider injection changes
- no secret enumeration API
- no `.env` export

## Acceptance

- Rhai scripts can request declared Rhai-targeted secrets
- missing required secrets block before script side effects where possible
- undeclared or wrong-target reads fail clearly
- values do not appear in Rhai errors or captured host output maps

## Completed

- Added `effigy::secret(name)` and `effigy::has_secret(name)` for declared
  Rhai-targeted vault secrets.
- Added invocation-local Rhai secret preflight so missing required Rhai secrets
  block before script execution.
- Rejected undeclared and wrong-target secret reads.
- Redacted known Rhai secret values from Rhai evaluation errors, host logs,
  process result maps, container exec result maps, and Effigy callback maps.
- Added Rhai tests for present, missing, undeclared, wrong-target, and error
  redaction cases.

## Validation

- Rhai host API tests
- redaction tests
- wrong-target tests
- `cargo check --all-targets`
- `git diff --check`

## Next Task

Execute `715` to add deploy/state/artifact secret injection.
