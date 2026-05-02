# 019 - V1 Release Contract And Compatibility Boundary

Generation: `g03`

Status: Planned
Owner: Platform
Created: 2026-05-02
Depends on: 018

## Problem

The runtime/container core is now hardened enough to justify `v1.0`
preparation, but Effigy still lacks an explicit stable release contract for
that transition.

The old `v0.x` release contract was good enough while core behavior was still
moving. It is not the right authority surface for a stable release line.

## Goal

Define the bounded `v1.0` release and compatibility contract for Effigy so
operators, automation, and future roadmap work know exactly what stability is
being promised.

## Scope

- replace the old `v0.x` release contract with an explicit `v1.0` contract for:
  - CLI invocation stability
  - config compatibility
  - JSON envelope and machine-contract stability
  - migration-note and deprecation rules
  - patch/minor/major expectations after `v1.0`
- define the stable operator surface Effigy is actually ready to guarantee:
  - local task routing
  - container-backed runtime behavior
  - release/install verification paths
  - bounded deployment export surfaces that are already shipped
- define what still remains outside the `v1.0` promise:
  - operator-owned Decodelabs production
  - backlog or exploratory channels
  - any internal-only authority surfaces that are not public contracts
- tie the contract to real validation commands and release gates

## Non-Goals

- publishing `v1.0` immediately
- widening product scope just to make the release contract look larger
- reopening the runtime/container hardening lane

## Exit Condition

This milestone is complete when:

- Effigy has one explicit `v1.0` release and compatibility contract
- the docs front doors stop relying on `v0.x` language for the future stable
  line
- the contract says plainly what is and is not inside the first `v1.0`
  promise

## Next Task

If this lane is promoted, start by replacing the draft `v0.x` release
contract with a first real `v1.0` authority surface before touching channel
execution.
