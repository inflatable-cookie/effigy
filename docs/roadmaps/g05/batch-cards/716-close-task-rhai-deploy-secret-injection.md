# 716 - Close Task Rhai Deploy Secret Injection

Roadmap: [`../004-task-rhai-and-deploy-secret-injection.md`](../004-task-rhai-and-deploy-secret-injection.md)
Strict lane: [`../../../specs/079-task-rhai-deploy-secret-injection-strict-lane.md`](../../../specs/079-task-rhai-deploy-secret-injection-strict-lane.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Close `g05.004` before container startup secret injection starts.

## Scope

- update command/reference docs
- update JSON examples where payloads changed
- update Rustdoc/module docs for secret resolution and injection
- record validation evidence
- close strict lane `079`
- move front doors to `g05.005`

## Non-Goals

- no container startup injection
- no `.env` export
- no provider secret provisioning

## Acceptance

- `g05.004` is complete
- strict lane `079` is complete
- task/Rhai/deploy/state/artifact secret consumption is documented
- next ready work is `g05.005`

## Completed

- Closed the task, Rhai, deploy, and state secret injection lane.
- Documented the current runtime injection rules in the secret management
  contract.
- Added Rustdoc for the target-scoped Rhai secret API and execution secret
  target field.
- Recorded validation evidence in the batch logs.
- Moved front doors to `g05.005`.

## Validation

- focused injection tests
- docs checks
- `cargo check --all-targets`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Open the first `g05.005` container startup secret injection card.
