# 073 Decide Demo Post-Browser-Live-Concurrent-Runner-Session-Parity Boundary

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after browser-owned live attached terminal
sessions reached bounded parity for single-process concurrent-runner-backed
interactive demos, while keeping the lane demo-scoped and still bounded away
from nested TUI embedding, generic process-manager work, and fresh browser
churn unless it clearly earns the next slot.

## In Scope

- decide whether the next value belongs in:
  - one more bounded browser/runtime parity follow-up
  - a runner-owned follow-up for richer concurrent-runtime truth
  - a pause from browser-terminal work now that the bounded live path covers
    both run-backed and single-process concurrent-runner demos
- preserve the no-nested-TUI rule
- leave one explicit ready card

## Out Of Scope

- implementing the next slice
- generic process-manager UI or multi-process demo browser controls
- embedding the concurrent TUI inside `effigy demo browser`
- desktop-client work

## Acceptance Criteria

- the next bounded slice after bounded browser live-session parity is explicit
- the decision stays demo-scoped rather than process-manager-scoped
- the lane remains anchored in one active ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the next move requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh operator intent

## Next Task

Execute this card to choose the next bounded slice after browser-owned live
attached terminal sessions reached bounded parity for run-backed and
single-process concurrent-runner-backed interactive demos.
