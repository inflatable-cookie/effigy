# 325 Implement Interactive Ownership Classification Foundation

Status: ready
Updated: 2026-05-01
Roadmap: `g03.010`
Spec: `docs/specs/023-interactive-session-ownership-and-lifecycle-convergence-strict-lane.md`

## Objective

Introduce one shared ownership/readiness model for interactive container-backed
sessions and apply it to the main handoff paths.

## In Scope

- add shared internal types for:
  - runtime ownership
  - session-readiness state
  - cleanup policy
- apply the shared model to workspace handoff and adopted-runtime shell entry
- move `stay_in_shell` cleanup decisions onto the same ownership model where
  they overlap with the shared path
- keep route/gateway readiness part of session-readiness completion
- add targeted parity tests around adopted-versus-session-owned cleanup

## Out Of Scope

- `exec` or non-shell activation changes
- bootstrap/Rhai embedded-runner convergence
- broad TUI or shell presentation redesign
- full attached `container up` convergence if it needs a follow-up slice

## Acceptance Criteria

- equivalent interactive sessions stop or preserve runtimes based on one shared
  ownership model rather than caller-local booleans
- adopted-but-not-fully-prepared runtimes are treated consistently
- workspace handoff and overlapping `stay_in_shell` paths no longer drift on
  cleanup semantics
- validation proves the shared ownership boundary without widening into
  embedded-runner work

## Validation

- targeted workspace/session ownership tests
- targeted handoff or adopted-runtime cleanup tests as needed
- `./target/debug/effigy docs check-paths docs/specs/023-interactive-session-ownership-and-lifecycle-convergence-strict-lane.md docs/specs/batch-cards/325-implement-interactive-ownership-classification-foundation.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

Execute `325`.
