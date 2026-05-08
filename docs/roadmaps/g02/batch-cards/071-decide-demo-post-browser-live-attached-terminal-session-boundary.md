# 071 Decide Demo Post-Browser-Live-Attached-Terminal-Session Boundary

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after browser-owned live attached terminal
sessions landed for browser-launched run-backed interactive demos, while
keeping the lane demo-scoped and bounded away from nested TUI embedding,
generic process-manager work, and fresh browser churn unless it clearly earns
the next slot.

## In Scope

- decide whether the next value belongs in:
  - one more bounded browser terminal follow-up on the new live attached path
  - a runner-owned follow-up that broadens backend parity behind the browser
  - a pause from browser-terminal work because the main operator gap is now
    closed enough
- preserve the no-nested-TUI rule
- leave one explicit ready card

## Out Of Scope

- implementing the next slice
- generic process-manager UI or multi-process demo sub-tabs
- embedding the concurrent TUI inside `effigy demo browser`
- desktop-client work

## Acceptance Criteria

- the next bounded slice after browser-owned live attached terminal sessions is
  explicit
- the decision stays demo-scoped rather than process-manager-scoped
- the lane remains anchored in one active ready card

## Decision

- do not spend the next slot on more browser chrome or control remapping
- do not pause browser-terminal work yet
- do not widen immediately to generic multi-process demo tabs or nested TUI
  embedding
- the next bounded slice is backend parity: browser-owned live attached
  terminal sessions for browser-launched single-process
  concurrent-runner-backed interactive demos
- multi-process concurrent-runner demos stay on the existing flattened
  projected terminal path until a later boundary says otherwise

## Why

- the new live attached terminal path fixed the main browser truth gap, but
  only for run-backed demos
- the next real product mismatch is backend parity inside the same browser
  terminal contract, not more tab or layout churn
- single-process concurrent-runner demos are the smallest honest expansion
  that preserves the no-nested-TUI rule
- widening directly to multi-process live embedding would collapse back into
  process-manager semantics and violate the bounded-slice rule

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the next move requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh operator intent

## Next Task

Execute
`072-implement-demo-browser-live-concurrent-runner-session-parity.md` to add
browser-owned live attached terminal sessions for browser-launched
single-process concurrent-runner-backed interactive demos while keeping
multi-process and nested-TUI cases on the existing projected path.
