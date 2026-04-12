# 075 Decide Demo Post-Concurrent-Runtime-Projection-Shape Boundary

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after runner-owned concurrent-runtime
projection-shape truth landed, while keeping the lane demo-scoped and still
bounded away from nested TUI embedding, generic process-manager work, and
fresh browser churn unless it clearly earns the next slot.

## In Scope

- decide whether the next value belongs in:
  - one more runner-owned concurrent-runtime truth slice
  - a bounded browser follow-up that consumes the richer shape honestly
  - a pause from this branch of demo work
- preserve the no-nested-TUI rule
- leave one explicit ready card

## Out Of Scope

- implementing the next slice
- generic process-manager UI
- multi-process browser tabs or panes by default
- embedding the concurrent TUI
- desktop-client work

## Acceptance Criteria

- the next bounded slice after projection-shape truth is explicit
- the decision stays demo-scoped instead of process-manager-scoped
- the lane remains anchored in one active ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the next move requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh operator intent

## Next Task

Execute this decision batch, then leave one explicit ready card instead of
widening directly into multi-process browser controls.
