# 327 Implement Shared Embedded-Runner Foundation

Status: archived
Updated: 2026-05-01
Roadmap: `g03.011`
Spec: `docs/specs/024-embedded-command-script-and-bootstrap-convergence-strict-lane.md`

## Objective

Introduce one shared embedded-runner entry for internal Effigy command replay
and move the main nested callers onto it.

## In Scope

- add one shared embedded-runner API that owns:
  - repo-targeted nested command execution
  - JSON propagation
  - recursion and handoff guard application
  - locking expectations
  - nested output projection handoff
- move bootstrap, Rhai, and run-array builtin re-entry onto that shared entry
- preserve caller-local presentation differences only where they are explicit
  and product-facing
- add targeted tests for nested command parity across the moved callers

## Out Of Scope

- regression-matrix drift guards
- redesigning bootstrap planning or Rhai surface scope
- execution binding or interactive shell ownership changes

## Acceptance Criteria

- one shared embedded-runner path exists
- bootstrap, Rhai, and run-array builtins no longer each own their own partial
  nested command semantics
- nested repo targeting, JSON handling, and recursion posture are consistent
  across the moved callers

## Validation

- targeted embedded command tests
- targeted bootstrap/Rhai/run-array parity tests as needed
- `./target/debug/effigy docs check-paths docs/specs/024-embedded-command-script-and-bootstrap-convergence-strict-lane.md docs/roadmaps/g03/batch-cards/327-implement-shared-embedded-runner-foundation.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/011-embedded-command-script-and-bootstrap-convergence.md`

## Next Task

Execute `328`.
