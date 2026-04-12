# 058 Decide Demo Post-Panel-First-Navigation Boundary

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after panel-first demo-browser navigation lands
without widening into browser churn, nested TUI embedding, or generic runtime
work.

## In Scope

- assess whether the next substantial value belongs in:
  - another bounded browser follow-up now that controls match the browser shape
  - a return to runner/query work because browser structure is coherent enough
  - a narrow cross-surface cleanup that sharpens the shipped browser contract
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- leave the lane with one explicit ready card

## Out Of Scope

- implementing browser terminal input or richer terminal transport controls
- embedding the concurrent TUI inside `effigy demo browser`
- generic managed-process UI or multi-process demo sub-tabs
- desktop-client work

## Acceptance Criteria

- the next slice is explicit and bounded
- the decision keeps the lane demo-scoped rather than process-manager-scoped
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
