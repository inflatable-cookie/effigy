# 324 Implement Exec Activation Convergence Foundation

Status: archived
Updated: 2026-05-01
Roadmap: `g03.009`
Spec: `docs/specs/022-execution-surface-convergence-strict-lane.md`

## Objective

Move `effigy exec` and exec aliases onto the shared non-shell runtime
activation contract.

## In Scope

- move explicit `exec` onto shared activation/prep
- move exec aliases onto the same activation contract
- keep named-container and default dev-container exec on one resolved surface
- preserve exec-specific raw command and output behavior
- add targeted parity tests for the shared activation boundary

## Out Of Scope

- interactive shell/session ownership convergence
- broader bootstrap or Rhai embedded-runner convergence
- projection redesign for exec output

## Acceptance Criteria

- stopped runtime plus `effigy exec` no longer fails just because the runtime
  was not already running
- exec aliases pick up the same activation behavior
- named-container and default dev-container exec use one shared activation
  contract
- validation proves activation parity without widening into shell lifecycle

## Validation

- targeted `exec_command` tests
- targeted activation/runtime tests as needed
- `./target/debug/effigy docs check-paths docs/specs/022-execution-surface-convergence-strict-lane.md docs/roadmaps/g03/batch-cards/324-implement-exec-activation-convergence-foundation.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

Promote `g03.010`.
