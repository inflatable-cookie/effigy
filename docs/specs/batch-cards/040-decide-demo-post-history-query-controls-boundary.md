# 040 Decide Demo Post-History-Query-Controls Boundary

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after shipped one-demo history query controls
without reopening browser churn or widening into generic timeline tooling.

## In Scope

- assess the shipped `demo history` outcome-filtering and ordinal-selection
  surface against the self-hosted demos
- decide whether any later history density should remain query-first or can
  safely move into a client/browser consumer
- leave the lane with one explicit follow-up card instead of free-continuing
  into UI density or generic analytics

## Out Of Scope

- implementing browser-side history panes, badges, or timelines in this batch
- widening into multi-demo history aggregation, analytics, or queueing
- broader runtime cancellation or desktop-client architecture work

## Acceptance Criteria

- the next history-related slice is explicit and bounded
- the decision preserves the one-demo query-first contract unless there is
  clear evidence for moving a narrow follow-up into a client surface
- the lane remains anchored in one active ready card

## Validation

- `git diff --check`
- `cargo run --bin effigy -- qa:docs`

## Stop Conditions

- the batch starts implementing UI changes instead of deciding the next slice
- the decision widens into generic timeline or analytics framework work
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute [`041-implement-demo-browser-history-handoff.md`](./041-implement-demo-browser-history-handoff.md)
to let the browser consume the settled one-demo history contract through a
bounded handoff without adding list density or in-browser timelines.
