# 038 Decide Demo Post-History-Drilldown Boundary

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after shipped historical-attempt drilldown
without reopening browser density churn or widening into generic timeline
tooling.

## In Scope

- assess the shipped `demo history` summary-plus-drilldown surface against the
  self-hosted demos
- decide whether the next bounded value belongs in:
  - `demo list`
  - `demo browser`
  - a deeper query/history contract
- keep the next slice focused on result-review usefulness rather than generic
  analytics or UI expansion

## Out Of Scope

- implementing browser timelines or list history badges in this decision batch
- multi-demo history aggregation, queueing, or generic analytics
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the next history-related slice is explicit and bounded
- the decision keeps list/browser density under control
- the lane remains anchored in one active ready card

## Validation

- `git diff --check`
- `cargo run --bin effigy -- qa:docs`

## Stop Conditions

- the batch starts implementing UI changes instead of deciding the next slice
- the decision widens into generic timeline or analytics framework work
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute [`039-implement-demo-history-query-controls.md`](./039-implement-demo-history-query-controls.md)
to add bounded history-query narrowing and selection ergonomics without
widening `demo list` or the browser.
