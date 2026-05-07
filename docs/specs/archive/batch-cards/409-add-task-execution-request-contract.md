# 409 - Add Task Execution Request Contract

Lane: [`041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md`](../041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Add the durable contract for `TaskExecutionRequestBuilder` and resolved
execution plans.

## Scope

- add `docs/contracts/013-task-execution-request-contract.md`
- update `docs/contracts/README.md`
- update `docs/contracts/json-schema-index.json` only if needed; no public JSON
  schema is expected in this card
- align the contract with shipped Rhai `exec::run(...)`,
  `TaskExecutionRequestBuilder`, and direct/embedded plan parity proofs
- no runner implementation changes

## Exit Condition

This card is complete when the task execution request contract names the
canonical owner, input model, resolution rules, Rhai expectations, and validation
direction for direct, bootstrap, Rhai, run-array, deferral, demo, and managed
flows.

## Closeout

Added `013-task-execution-request-contract.md` and linked it from the contracts
index.

No JSON schema index change was needed because resolved execution plans remain
internal in this round.

## Next Task

Widen the execution convergence contract to reference the request builder.
