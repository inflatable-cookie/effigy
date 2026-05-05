# 410 - Widen Execution Convergence Contract For Request Builder

Lane: [`041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md`](../041-contract-promotion-public-cleanup-breaks-and-closeout-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Update `009-execution-surface-convergence.md` so the shipped
`TaskExecutionRequestBuilder` is the named authority for execution request and
plan construction.

## Scope

- update the ownership map in `009`
- add request-builder rules for direct and embedded surfaces
- name Rhai container-targeted execution as a required shared surface
- keep public CLI behavior unchanged
- no implementation changes

## Exit Condition

This card is complete when `009` points at `013` for canonical task request
construction and no longer describes execution request construction as
caller-local ownership.

## Closeout

Updated `009-execution-surface-convergence.md` to name
`TaskExecutionRequestBuilder` as the shared request/plan authority and added
Rhai `exec::run(...)` to the covered matrix.

The contract now points at `011`, `012`, and `013` as the durable authority set
for runtime/context/container/execution convergence.

## Next Task

Update the package map for the new core crates.
