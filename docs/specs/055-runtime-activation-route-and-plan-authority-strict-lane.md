# 055 - Runtime Activation Route And Plan Authority Strict Lane

Roadmap: [`g04.013`](../roadmaps/g04/013-runtime-activation-route-and-plan-authority.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Make runtime activation plans carry honest route identity and reduce duplicated
activation request construction across runner callers.

## Hard Boundaries

- no release work
- no `.github/workflows/` edits
- keep public CLI behavior unchanged
- keep activation side effects in runner/runtime-prep stage modules
- do not start broad manager cleanup from this lane

## Current Ready Card

[`581-close-runtime-activation-route-authority.md`](./batch-cards/581-close-runtime-activation-route-authority.md)

## Execution Chain

- `579` complete: add route selection to activation requests and set obvious
  caller routes
- `580` complete: centralize runner activation-plan construction
- `581` ready: close route authority and hand off to data plans

## Focus

- add route selection to `RuntimeActivationRequest`
- stop silently defaulting non-task activation plans to `Task`
- mark exec, managed, deferral, DB seed, and standard task activation routes
- add focused tests for route identity
- leave shared builder extraction for the next card after route identity is
  real

## Exit Condition

This lane closes when route identity is explicit, repeated request construction
has a selected consolidation path, and `g04.014` is ready.

## Next Task

Card
[`582-wire-bootstrap-db-seed-through-data-seed-plan.md`](./batch-cards/582-wire-bootstrap-db-seed-through-data-seed-plan.md).
