# 005 - Execution Path Unification And Runtime Prep

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-04-30
Depends on: 004

## Problem

Effigy still has too many ways to prepare a container runtime.

The immediate `stay_in_shell` split was collapsed, but runtime preparation is
still scattered across:

- managed container lifecycle
- standard routed exec readiness
- workspace shell handoff

That leaves too much room for drift in semantics, ordering, warnings, and
future fixes.

## Goal

Collapse container runtime preparation behind one shared execution-prep layer
used by both managed and standard surfaces.

## Scope

- extract one shared runtime-prep unit for handoff and routed exec
- centralize container handoff environment ownership
- centralize alias reconciliation and sibling-service bring-up
- make surface-specific behavior explicit only where the product genuinely
  differs
- remove duplicated seeded-session and handoff decision code where it still
  exists

## Non-Goals

- merging managed TUI presentation with standard shell execution
- removing legitimate differences between attached and detached lifecycle
  modes
- changing task routing policy

## Exit Condition

This milestone is complete when managed and standard execution both consume
the same runtime-prep contract and new runtime fixes land in one place by
default.

## Next Task

Promote `g03.006` and use the shared runtime-prep layer now anchored in:

- [`../../../src/runner/container_runtime_prep.rs`](../../../src/runner/container_runtime_prep.rs)
- [`../../contracts/005-container-runtime-contract.md`](../../contracts/005-container-runtime-contract.md)

to define the backend capability matrix and targeted compatibility coverage.
