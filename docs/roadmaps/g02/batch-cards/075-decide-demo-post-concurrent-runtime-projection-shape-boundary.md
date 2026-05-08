# 075 Decide Demo Post-Concurrent-Runtime-Projection-Shape Boundary

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

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

## Decision

- do not spend the next slot on browser chrome or multi-process browser
  controls
- do not pause this branch yet
- the next bounded slice is one more runner-owned concurrent-runtime truth
  layer
- specifically: add bounded projected-runtime process summary facts for
  concurrent-runner demos that stay on the flattened path, so clients can tell
  what processes sit behind one demo-owned projected terminal/session without
  turning the demo browser into a process manager

## Why

- `projection_shape` answered whether one live terminal is honest
- the next real gap is what a projected multi-process runtime actually contains
- browser follow-up now would still need to guess or stay vague about the
  managed processes behind that projection
- one bounded runner-owned process summary keeps later browser work honest
  without widening into generic process-manager UI

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the next move requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh operator intent

## Next Task

Execute
`076-implement-demo-concurrent-runtime-projected-process-summary-contract.md`
to add bounded runner-owned process summary facts for projected
concurrent-runner demos.
