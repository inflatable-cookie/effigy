# 046 Decide Demo Post-Browser-Terminal-View Boundary

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after the shipped browser terminal view without
widening into nested TUI embedding or ad-hoc browser churn.

## In Scope

- assess whether the next terminal-related value belongs in:
  - demo-scoped browser presentation convergence such as `Overview`,
    `History`, `Terminal`, and `Artifacts` tabs
  - deeper runner-owned active-terminal input/session contract work
  - another tightly bounded one-demo browser follow-up
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- leave the lane with one explicit ready card

## Out Of Scope

- implementing tabs or input forwarding in this decision batch
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process UI
- retained-history replay as an interactive terminal
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the next terminal/browser slice is explicit and bounded
- the decision keeps demo-browser terminal consumption demo-scoped rather than
  process-manager-scoped
- the lane remains anchored in one active ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing tabs or terminal input instead of deciding
- the decision requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute [`047-implement-demo-active-terminal-input-contract.md`](./047-implement-demo-active-terminal-input-contract.md)
to deepen the runner-owned active demo terminal/session contract with bounded
input-forwarding semantics before any browser tab convergence work.
