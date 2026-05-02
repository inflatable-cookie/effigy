# 013 - Runtime Session Context And Runtime Ownership Hardening

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-02
Depends on: 004, 005, 007, 009, 010, 011, 012

## Problem

Effigy's runtime lifecycle rules still depend too much on ambient env flags
 and caller-local branching.

That works, but it is hard to reason about and easy to regress when a new
 entrypoint is added. Lease refresh, stop-on-exit, bootstrap handoff, and
 interactive ownership should be explicit runtime policy, not hidden process
 state.

## Goal

Replace env-driven runtime control with one typed activation and session
 context model that owns lease, handoff, and ownership semantics across the
 runner.

## Scope

- define one typed runtime/session context for:
  - activation purpose
  - interactive versus non-interactive activation
  - ownership mode
  - lease policy
  - handoff policy
- thread that context through the main runtime entrypoints:
  - bootstrap task dispatch
  - bootstrap start handoff
  - public workspace sessions
  - seeded task shells
  - routed container activation
  - explicit `effigy exec`
- remove internal dependence on runtime-control env flags where they currently
  steer behavior instead of exposing real operator config
- add focused parity tests proving the typed path for:
  - bootstrap no-lease setup phases
  - bootstrap shell handoff stop-on-exit
  - direct workspace ownership
  - seeded shell ownership
  - non-shell exec activation

## Non-Goals

- rewriting container assembly or compose generation
- changing public catalog or bundle contracts
- broad runner crate extraction

## Exit Condition

This milestone is complete.

`334` landed the first typed runtime/session context and moved the main
 lifecycle seams onto it:

- bootstrap setup work
- bootstrap public-workspace handoff
- public workspace sessions
- seeded shell ownership overlap
- routed container activation
- deferred container activation
- explicit `effigy exec`

## Next Task

Promote `g03.014`.
