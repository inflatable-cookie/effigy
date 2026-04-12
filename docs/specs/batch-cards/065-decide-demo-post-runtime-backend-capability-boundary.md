# 065 Decide Demo Post-Runtime-Backend-Capability Boundary

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after runner-owned demo runtime backend and
capability reporting landed, while keeping the lane demo-scoped and bounded
away from browser churn, nested TUI embedding, and generic process-manager
work.

## In Scope

- decide whether the next value belongs in:
  - one richer runner backend implementation slice on top of the new contract
  - one narrow browser consumer follow-up that uses the new backend facts
  - a pause from terminal/runtime work because the contract is coherent enough
- preserve the no-nested-TUI rule for concurrent-runner-backed demos
- leave one explicit ready card

## Out Of Scope

- implementing the next richer runtime slice
- browser layout or control redesign
- generic process-manager UI or multi-process demo sub-tabs
- desktop-client work

## Acceptance Criteria

- the next bounded slice after backend/capability reporting is explicit
- the decision stays demo-scoped rather than process-manager-scoped
- the lane remains anchored in one active ready card

## Result

- do not take a browser consumer follow-up next
- do not pause terminal/runtime work yet
- the next bounded slice is one richer runner backend implementation:
  flatten concurrent-runner-backed demos behind the shipped demo session
  contract without nested TUI launch

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the next move requires nested TUI launch to stay coherent
- the next move becomes generic runtime-manager work instead of demo-scoped
  runner/client follow-up

## Next Task

Execute [`066-implement-demo-concurrent-runner-session-projection.md`](./066-implement-demo-concurrent-runner-session-projection.md)
to project concurrent-runner-backed demos through the existing demo session
contract without launching a nested TUI.
