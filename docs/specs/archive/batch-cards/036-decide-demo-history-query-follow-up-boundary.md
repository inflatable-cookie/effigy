# 036 Decide Demo History Query Follow-Up Boundary

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after the shipped `demo history` query surface
without reopening browser churn or widening into generic timeline tooling.

## In Scope

- assess the shipped `demo history` surface against the self-hosted demos
- decide whether the next bounded slice belongs in:
  - `demo list`
  - `demo browser`
  - a deeper history/query contract
- record the next explicit ready card

## Out Of Scope

- implementing browser timeline rendering
- widening into multi-demo history aggregation
- broader runtime queueing, cancellation, or desktop-client work

## Acceptance Criteria

- the next history-related slice is explicit and bounded
- the decision keeps browser/list density under control
- the lane remains anchored in one active ready card

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the batch starts implementing UI changes instead of deciding the next slice
- the decision expands into generic analytics or timeline framework work
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute [`037-implement-demo-history-attempt-drilldown.md`](./037-implement-demo-history-attempt-drilldown.md)
to deepen the dedicated `demo history` surface around stable attempt selection
and one-attempt result inspection without widening `demo list` or the browser.
