# 063 Decide Demo Post-Terminal-Resize-Contract Boundary

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after runner-owned demo terminal resize semantics
land without drifting back into browser churn, nested TUI embedding, or
generic runtime-manager work.

## In Scope

- assess whether the next substantial value belongs in:
  - deeper runner-owned terminal fidelity for detached/browser-consumed demos
  - one more narrow browser terminal follow-up on top of the shipped resize
    contract
  - a pause from browser terminal work because the contract is coherent enough
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- leave the lane with one explicit ready card

## Out Of Scope

- implementing the next terminal fidelity slice
- browser layout or control redesign
- embedding the concurrent TUI inside `effigy demo browser`
- generic managed-process UI or multi-process demo sub-tabs
- desktop-client work

## Acceptance Criteria

- the next slice after active demo terminal resize semantics is explicit and
  bounded
- the decision keeps the lane demo-scoped rather than process-manager-scoped
- the lane remains anchored in one active ready card

## Result

- browser terminal work is coherent enough to pause again after terminal
  emulation, input, and resize all landed
- the next bounded slice is runner-owned runtime backend capability reporting
  for active demo sessions and inspect surfaces
- the next card should make richer runtimes, including concurrent-runner-backed
  demos, project honest demo-scoped capability facts without nested TUI launch

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the decision requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute [`064-implement-demo-runtime-backend-capability-contract.md`](./064-implement-demo-runtime-backend-capability-contract.md)
to add bounded runtime backend and capability facts for active demo sessions so
future richer runtimes stay demo-scoped and do not force meaning through the
browser.
