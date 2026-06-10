# Task Execution Request Contract

Date: 2026-05-05

## Change

Completed card `409` and added
`docs/contracts/013-task-execution-request-contract.md`.

## Result

The contract names `effigy_execution` as the owner for
`TaskExecutionRequestBuilder`, resolved execution plans, surface labels,
runtime policy, handoff policy, cleanup policy, output mode, and environment
plans.

It also captures the Rhai rule from the DecodeLabs mysql seed bug: scripts
should request container execution by typed intent instead of choosing
host-process versus container exec locally.

## Validation

No JSON schema update was needed because resolved plans remain internal.

## Next

Complete card `410`.
