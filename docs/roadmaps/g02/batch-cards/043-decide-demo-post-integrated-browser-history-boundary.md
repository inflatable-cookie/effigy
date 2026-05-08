# 043 Decide Demo Post-Integrated-Browser-History Boundary

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after the shipped integrated browser history view
without reopening broad browser churn or widening into generic timeline
tooling.

## In Scope

- assess whether the integrated one-demo history view is enough browser-side
  consumption for now
- decide whether the next bounded value belongs in:
  - deeper one-demo browser activation from retained attempts
  - renewed runner/query work
  - another tightly bounded client follow-up
- leave the lane with one explicit ready card

## Out Of Scope

- widening into multi-demo history aggregation, analytics, or queueing
- adding `demo list` retained-history density, badges, or grouped summaries
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the next history/browser slice is explicit and bounded
- the decision preserves the settled one-demo `demo history` contract as the
  source of truth
- the lane remains anchored in one active ready card

## Validation

- `git diff --check`
- `cargo run --bin effigy -- qa:docs`

## Stop Conditions

- the batch starts implementing browser changes instead of deciding the next
  slice
- the decision widens into generic timeline or analytics framework work
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute [`044-implement-demo-active-terminal-session-handoff.md`](./044-implement-demo-active-terminal-session-handoff.md)
to add a runner-owned active demo terminal/session contract before any
tabbed browser terminal integration.
