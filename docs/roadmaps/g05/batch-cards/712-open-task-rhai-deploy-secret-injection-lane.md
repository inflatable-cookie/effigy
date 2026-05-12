# 712 - Open Task Rhai Deploy Secret Injection Lane

Roadmap: [`../004-task-rhai-and-deploy-secret-injection.md`](../004-task-rhai-and-deploy-secret-injection.md)
Contract: [`../../../contracts/032-secret-and-local-config-management-contract.md`](../../../contracts/032-secret-and-local-config-management-contract.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Open the `g05.004` implementation lane for consuming unlocked vault secrets in
tasks, Rhai scripts, deploy provider packages, state, and artifact workflows.

## Scope

- create the strict lane for `g05.004`
- define the no-plaintext-file injection boundary
- split task, Rhai, deploy, state, and artifact injection into follow-up cards
- preserve `.env.schema` compatibility
- preserve value redaction in reports and errors

## Non-Goals

- no container startup injection
- no compatibility `.env` export
- no provider secret provisioning
- no undeclared-key reads

## Acceptance

- [x] strict lane exists for `g05.004`
- [x] implementation cards are sequenced
- [x] task/Rhai/deploy injection boundaries are explicit
- [x] runtime injection remains blocked until the lane is open

## Outcome

Opened strict lane `079` for `g05.004`. Injection is scoped to declared targets
and split into task, Rhai, and deploy/state/artifact follow-up cards.

## Validation

- docs path checks
- `git diff --check`

## Next Task

Execute `713` to add task secret injection.
