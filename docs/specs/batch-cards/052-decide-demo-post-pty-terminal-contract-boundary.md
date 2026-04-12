# 052 Decide Demo Post-PTY-Terminal-Contract Boundary

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after PTY-backed demo terminal/session semantics
land without widening into nested TUI embedding or generic runtime churn.

## In Scope

- assess whether the next terminal-related value belongs in:
  - deeper runner-owned input/session semantics on top of the shipped PTY path
  - bounded browser terminal convergence on top of the richer runner contract
  - demo-scoped tab convergence such as `Overview`, `History`, `Terminal`, and
    `Artifacts`
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- leave the lane with one explicit ready card

## Out Of Scope

- implementing browser tabs, browser input, or broader runtime controls in
  this decision batch
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process UI
- retained-history replay as an interactive terminal
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the next terminal/browser slice is explicit and bounded
- the decision keeps demo-browser terminal work demo-scoped rather than
  process-manager-scoped
- the lane remains anchored in one active ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing tabs or browser input instead of deciding
- the decision requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute the ready follow-up selected by this boundary decision.
