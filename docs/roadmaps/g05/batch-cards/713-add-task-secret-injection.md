# 713 - Add Task Secret Injection

Roadmap: [`../004-task-rhai-and-deploy-secret-injection.md`](../004-task-rhai-and-deploy-secret-injection.md)
Strict lane: [`../../../specs/079-task-rhai-deploy-secret-injection-strict-lane.md`](../../../specs/079-task-rhai-deploy-secret-injection-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Inject declared task-targeted secrets into task process environments without
shell command leakage.

## Scope

- resolve `[secrets]` declarations with `targets = ["tasks"]`
- unlock the local vault for task execution when required
- add declared secret values to child process environment maps
- block missing required task secrets before process spawn
- redact secret values from task output metadata, errors, and JSON reports
- preserve existing `.env.schema` resolution behavior

## Non-Goals

- no Rhai API
- no deploy/state/artifact injection
- no container startup injection
- no `.env` export
- no provider secret provisioning

## Acceptance

- task commands receive declared task secrets through process env APIs
- undeclared keys cannot be read
- missing required task secrets block before command execution
- values do not appear in captured task JSON envelopes
- existing `.env.schema` sensitive handling still works

## Completed

- Added task-targeted vault secret resolution for `[secrets.keys.*]` entries
  with `targets = ["tasks"]`.
- Injected resolved values through the existing task secret environment path
  used by `.env.schema` sensitive values.
- Added pre-spawn blockers for missing required task secrets.
- Redacted known secret values from captured host, routed-container, and inline
  task JSON output.
- Added task execution tests for injection, missing required blockers, and
  `.env.schema` compatibility.

## Validation

- task execution tests proving env injection
- missing-secret blocker tests
- redaction tests
- `.env.schema` compatibility tests
- `cargo check --all-targets`
- `git diff --check`

## Next Task

Execute `714` to add the Rhai secret API.
