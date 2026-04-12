# 056 Decide Demo Post-Browser-Tab-Convergence Boundary

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after demo-scoped browser tab convergence lands
without widening into nested TUI embedding, browser bloat, or generic runtime
churn.

## In Scope

- assess whether the next value belongs in:
  - bounded browser-side interaction on top of the converged tab surface
  - another narrowly bounded browser polish or consumption slice
  - a return to runner/query work if browser-side value has flattened
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- leave the lane with one explicit ready card

## Out Of Scope

- implementing browser input, nested TUI embedding, or broader runtime controls
- multi-process demo sub-tabs or generic managed-process UI
- retained-history replay as an interactive terminal
- broad runtime cancellation or desktop-client work

## Acceptance Criteria

- the next slice is explicit and bounded
- the decision keeps demo-browser work demo-scoped rather than
  process-manager-scoped
- the lane remains anchored in one active ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the decision requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute the ready follow-up selected by this boundary decision.
