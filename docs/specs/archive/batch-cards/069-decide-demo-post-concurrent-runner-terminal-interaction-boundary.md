# 069 Decide Demo Post-Concurrent-Runner Terminal Interaction Boundary

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after concurrent-runner-backed demos can expose
bounded terminal interaction through the demo session contract, while keeping
the lane demo-scoped and bounded away from browser churn, nested TUI
embedding, and generic process-manager work.

## In Scope

- decide whether the next value belongs in:
  - one more runner-owned concurrent-runtime fidelity slice
  - one narrow browser/client consumer follow-up on the shipped interaction projection
  - a pause from terminal/runtime work because the backend boundary is now coherent enough
- preserve the flattened no-nested-TUI rule for concurrent-runner-backed demos
- leave one explicit ready card

## Out Of Scope

- implementing the next slice
- browser layout or control redesign
- generic process-manager UI or multi-process demo sub-tabs
- desktop-client work

## Acceptance Criteria

- the next bounded slice after concurrent-runner interaction projection is explicit
- the decision stays demo-scoped rather than process-manager-scoped
- the lane remains anchored in one active ready card

## Result

- recover the browser-terminal authority chain:
  - shipped browser terminal behavior is a vt-backed replay/input surface
  - it is not yet a browser-owned live attached terminal session
- do not take more runner-only concurrent-runtime fidelity next
- do not pause terminal/browser work yet
- the next bounded slice is browser-owned live attached terminal attachment
  for browser-launched run-backed interactive demos
- preserve the no-nested-TUI rule by keeping concurrent-runner-backed demos
  on the flattened projected path for now

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the next move requires nested TUI launch to stay coherent
- the next move becomes generic process-manager work instead of demo-scoped follow-up

## Next Task

Execute [`070-implement-demo-browser-live-attached-terminal-session.md`](./070-implement-demo-browser-live-attached-terminal-session.md)
to replace browser terminal replay with a browser-owned live attached terminal
session for browser-launched run-backed interactive demos.
