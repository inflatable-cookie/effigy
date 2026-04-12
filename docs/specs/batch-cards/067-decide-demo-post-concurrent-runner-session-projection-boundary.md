# 067 Decide Demo Post-Concurrent-Runner Session Projection Boundary

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after concurrent-runner-backed demos can project
through the demo session contract, while keeping the lane demo-scoped and
bounded away from browser churn, nested TUI embedding, and generic
process-manager work.

## In Scope

- decide whether the next value belongs in:
  - one more runner-owned concurrent-runtime fidelity slice
  - one narrow browser/client consumer follow-up on the shipped projection
  - a pause from terminal/runtime work because the backend boundary is now coherent enough
- preserve the flattened no-nested-TUI rule for concurrent-runner-backed demos
- leave one explicit ready card

## Out Of Scope

- implementing the next slice
- browser layout or control redesign
- generic process-manager UI or multi-process demo sub-tabs
- desktop-client work

## Acceptance Criteria

- the next bounded slice after concurrent-runner projection is explicit
- the decision stays demo-scoped rather than process-manager-scoped
- the lane remains anchored in one active ready card

## Result

- do not take a browser/client follow-up next
- do not pause terminal/runtime work yet
- the next bounded slice is runner-owned concurrent-runner terminal
  interaction projection:
  - input forwarding
  - resize semantics
  - still flattened behind the demo-scoped session contract

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the next move requires nested TUI launch to stay coherent
- the next move becomes generic process-manager work instead of demo-scoped follow-up

## Next Task

Execute [`068-implement-demo-concurrent-runner-terminal-interaction-projection.md`](./068-implement-demo-concurrent-runner-terminal-interaction-projection.md)
to add bounded input and resize projection for concurrent-runner-backed demos
through the demo session contract.
