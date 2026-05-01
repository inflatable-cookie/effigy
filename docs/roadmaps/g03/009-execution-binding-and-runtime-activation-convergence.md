# 009 - Execution Binding And Runtime Activation Convergence

Generation: `g03`

Status: Planned
Owner: Platform
Created: 2026-05-01
Depends on: 007, 008

## Problem

Binding resolution and runtime activation still have too many caller-shaped
paths.

The largest remaining gap is that `effigy exec` still bypasses the shared task
activation contract and only checks whether the container is already running.

That leaves startup, exec readiness, gateway and route reconciliation, alias
reconciliation, and lease behavior stronger on some task paths than on other
container-backed execution surfaces that mean the same thing to users.

## Goal

Create one shared binding-resolution and runtime-activation contract for
container-backed execution surfaces.

## Scope

- add shared internal types for:
  - `ExecutionSurfaceKind`
  - `ExecutionBindingResolution`
  - `ActivationRequest`
  - `ActivationOutcome`
- resolve execution binding once per surface entry instead of re-deriving it
  ad hoc downstream
- move `effigy exec` and exec aliases onto the shared activation contract
- keep command construction and output projection surface-specific where needed
- keep runtime activation responsible for:
  - startup
  - exec readiness
  - gateway and route reconciliation
  - alias reconciliation
  - lease refresh

## Non-Goals

- redesigning the compose backend model
- merging shell/session ownership into non-shell task activation
- changing what `exec` runs inside the target service

## Exit Condition

This milestone is complete when:

- stopped runtime plus `effigy exec` behaves consistently with stopped runtime
  plus container-backed task execution
- named-container and default dev-container exec share the same activation
  contract once targeted
- remaining task-versus-exec differences are limited to command/projection
  semantics rather than lifecycle effects

## Next Task

Promote `g03.010`.

Once non-shell activation is shared, converge the remaining interactive shell
and attached-session ownership rules.
