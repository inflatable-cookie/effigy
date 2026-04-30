# 004 - Container Runtime Contract And Alias Surface

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-04-30
Depends on: 016, 047, 056

## Problem

Effigy's container-backed task contract is still too implicit.

Core runtime guarantees such as:

- service alias resolution
- workspace handoff behavior
- routed exec readiness
- container-local `effigy` availability

currently depend on a mix of compose behavior, runtime repair, and
surface-specific orchestration.

That is how the same Decodelabs site could work under `effigy dev` but fail
under `effigy bootstrap`.

## Goal

Define one explicit runtime contract for container-backed task execution,
including the supported alias surface and the fallback behavior Effigy owns
when the compose backend does not deliver that contract directly.

## Scope

- document the required runtime guarantees for container-backed tasks
- define which alias classes Effigy guarantees inside containers
- define whether those guarantees apply to primary service only or to every
  task-exec service
- define when runtime reconciliation runs before handoff or exec
- define the ownership boundary between compose-declared state and
  Effigy-repaired state

## Non-Goals

- replacing the current gateway model
- redesigning bundle-level DNS naming
- broad Decodelabs app bootstrap cleanup outside the runtime contract

## Exit Condition

This milestone is complete when Effigy has a written, testable runtime
contract for container-backed tasks and the alias behavior no longer depends
on undocumented surface-specific assumptions.

## Next Task

Promote `g03.005` and use
[`../../contracts/005-container-runtime-contract.md`](../../contracts/005-container-runtime-contract.md)
as the contract anchor for runtime-prep unification.
