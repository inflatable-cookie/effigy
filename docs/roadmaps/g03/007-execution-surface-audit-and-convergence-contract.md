# 007 - Execution Surface Audit And Convergence Contract

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-01
Depends on: 004, 005, 006

## Problem

Effigy has already collapsed some obvious runtime drift, but the broader
execution surface is still too path-shaped.

The same user intent can still pick up different behavior depending on whether
it arrives through:

- managed `dev`
- managed task execution
- standard routed tasks
- deferral
- bootstrap `run`
- bootstrap `start`
- `effigy workspace`
- `stay_in_shell`
- `effigy exec`
- exec aliases
- `effigy container shell`
- run-array builtins
- Rhai command dispatch
- demo task re-entry

That leaves product semantics partially hidden behind caller path instead of one
explicit contract.

## Goal

Define one execution-surface convergence contract and one responsibility matrix
for the shared runtime, routing, embedded-dispatch, and session semantics Effigy
owns.

## Scope

- add a new execution-surface convergence contract doc
- classify every relevant execution surface on:
  - repo targeting source
  - binding resolution source
  - runtime activation source
  - gateway and alias behavior
  - lease behavior
  - shell or session ownership
  - cleanup ownership
  - JSON and output projection behavior
  - inline workspace container support
  - embedded recursion and handoff handling
- record one owning module for every shared responsibility
- record the deliberate surface-specific exceptions that are allowed to remain

## Non-Goals

- implementing the full convergence work in this milestone
- reopening generic crate-boundary cleanup
- widening deployment-export scope

## Exit Condition

This milestone is complete when Effigy has one explicit, decision-complete
execution-surface contract and the remaining drift points are classified as
either:

- required shared behavior to converge in later milestones
- or deliberate exceptions with named ownership

## Outcome

This milestone is now complete.

The execution-surface convergence contract and responsibility matrix now live
in:

- [`../../contracts/009-execution-surface-convergence.md`](../../contracts/009-execution-surface-convergence.md)

That contract now names:

- the covered execution surfaces
- the shared ownership map
- the required convergence rules
- the allowed differences
- the known intentional exceptions
- the parity-proof direction for later milestones

## Next Task

Promote `g03.008`.

Use the new execution-surface contract and matrix to centralize repo targeting
and embedded Effigy command dispatch before moving on to runtime activation
convergence.
