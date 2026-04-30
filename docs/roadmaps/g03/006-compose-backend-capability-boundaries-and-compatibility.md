# 006 - Compose Backend Capability Boundaries And Compatibility

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-04-30
Depends on: 004, 005

## Problem

Effigy currently treats compose backends as more equivalent than they really
are.

The Colima + `nerdctl compose` path has already shown that assumptions around:

- network alias materialization
- bind-mount auto-creation
- exec readiness after recreate

can drift from Docker Compose behavior while still looking correct at the
generated manifest layer.

## Goal

Make backend capability boundaries explicit, testable, and owned by Effigy
instead of relying on accidental parity.

## Scope

- define which runtime features Effigy requires from a compose backend
- define which gaps Effigy repairs itself
- separate gateway-route derivation from runtime alias reconciliation where
  that improves ownership clarity
- add regression coverage for backend-sensitive runtime behavior
- document the compatibility model for supported local backends

## Non-Goals

- adding new local container backends
- full end-to-end production deployment work
- provider-export roadmap changes

## Exit Condition

This milestone is complete when backend assumptions are visible in code and
docs, fallback behavior is intentional, and runtime regressions like the CBS
bootstrap failure are covered by targeted compatibility tests.

## Next Task

Return the queue to `g03.001`.

Use the compatibility contract and shared runtime-prep coverage now anchored
in:

- [`../../contracts/006-compose-backend-compatibility.md`](../../contracts/006-compose-backend-compatibility.md)
- [`../../../src/runner/container_runtime_prep.rs`](../../../src/runner/container_runtime_prep.rs)

as the closed local-runtime basis while the deployment-export lane starts.
