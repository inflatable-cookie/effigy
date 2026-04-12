# 042 Decide Demo Post-Browser-History-Handoff Boundary

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after the shipped browser history handoff
without reopening browser churn or widening into generic timeline tooling.

## In Scope

- assess whether the shipped browser history handoff is enough browser-side
  consumption for now
- decide whether any next bounded value belongs in:
  - deeper browser consumption
  - renewed runner/query work
  - another tightly bounded client follow-up
- leave the lane with one explicit ready card

## Out Of Scope

- implementing browser-side retained history tables, badges, or timelines in
  this decision batch
- widening into multi-demo history aggregation, analytics, or queueing
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the next history-related slice is explicit and bounded
- the decision preserves the settled one-demo history contract as the source
  of truth
- the lane remains anchored in one active ready card

## Validation

- `git diff --check`
- `cargo run --bin effigy -- qa:docs`

## Stop Conditions

- the batch starts implementing UI changes instead of deciding the next slice
- the decision widens into generic timeline or analytics framework work
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute the ready follow-up selected in this decision batch so the lane stays
bounded and explicit.
