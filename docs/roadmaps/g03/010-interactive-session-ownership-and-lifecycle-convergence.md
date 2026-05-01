# 010 - Interactive Session Ownership And Lifecycle Convergence

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-01
Depends on: 007, 009

## Problem

Interactive shell and attached-session ownership still depends too much on
local booleans and caller history.

The remaining surfaces still owning parts of this behavior include:

- `effigy workspace`
- managed handoff
- `stay_in_shell`
- `effigy container up --attach`
- adopted-runtime shell/session flows

That makes teardown, adopted-runtime handling, and readiness completion harder
to reason about than they should be.

## Goal

Create one interactive session ownership model and use it across shell-backed
execution surfaces.

## Scope

- add shared internal types for:
  - `RuntimeOwnership`
  - `SessionReadinessState`
  - `CleanupPolicy`
- make shell/session cleanup derive from ownership state rather than local
  booleans
- make route and gateway readiness part of session-readiness completion
- treat adopted-but-not-fully-prepared runtimes consistently
- keep `on_task_exit` as lifecycle policy rather than implicit ownership

## Non-Goals

- merging interactive shells with non-interactive task activation
- redesigning attached TUI presentation
- changing direct operator `container down` semantics

## Exit Condition

This milestone is complete when equivalent interactive sessions produce
equivalent cleanup behavior regardless of whether the runtime was first touched
by:

- bootstrap
- task auto-activation
- explicit `dev`
- workspace handoff

## Next Task

Promote `g03.011`.

The first ownership foundation closed the overlapping direct workspace and
seeded-task shell paths. Attached `container up --attach` remains a deliberate
operator lifecycle surface, so it is not the next widening seam for this
roadmap.
