# 016 - Container And Runtime Error Taxonomy And Diagnostics

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-02
Depends on: 013, 014, 015

## Problem

Important runtime and container failures still collapse into generic
 task-invocation strings.

That makes failures harder to diagnose, weakens contract-level testing, and
 hides the real subsystem boundaries that should become stable before `v1.0`.

## Goal

Introduce typed error families for the runtime/container core and sharpen the
 user-facing diagnostics built on top of them.

## Scope

- define typed failure families for:
  - runtime activation
  - workspace handoff/session ownership
  - host-container lease policy
  - compose/container policy application
  - gateway reconciliation where it crosses this runtime surface
- reduce use of generic string buckets in the runtime/container path
- improve error-to-display mapping so operators can distinguish:
  - bad config
  - unsupported runtime surface
  - transient runtime prep failure
  - container assembly or policy conflict
  - ownership or cleanup misuse
- add focused tests that assert error categories for the main brittle seams

## Non-Goals

- broad CLI copy polish
- TUI redesign
- localization or presentation-system work

## Exit Condition

This milestone is complete when:

- the core runtime/container path has typed error families instead of
  string-first failure handling
- generic task-invocation strings are no longer the dominant failure path for
  these subsystems
- the main brittle seams have category-level error tests

## Next Task

Closed. Promote `g03.017`.
