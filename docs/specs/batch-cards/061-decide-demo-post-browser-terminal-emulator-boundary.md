# 061 Decide Demo Post-Browser-Terminal-Emulator Boundary

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after embedded demo-browser terminal emulation
lands without sliding into browser churn, nested TUI embedding, or generic
runtime-manager work.

## In Scope

- assess whether the next substantial value belongs in:
  - deeper runner-owned terminal/session fidelity such as resize or richer live
    transport semantics
  - one more narrow browser follow-up now that terminal emulation is real
  - a pause from browser work because the terminal surface is coherent enough
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- leave the lane with one explicit ready card

## Out Of Scope

- implementing the next terminal fidelity slice
- embedding the concurrent TUI inside `effigy demo browser`
- generic managed-process UI or multi-process demo sub-tabs
- desktop-client work

## Acceptance Criteria

- the next slice after embedded browser terminal emulation is explicit and
  bounded
- the decision keeps the lane demo-scoped rather than process-manager-scoped
- the lane remains anchored in one active ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing instead of deciding
- the decision requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh evidence

## Next Task

Execute this card to choose the next bounded slice after browser terminal
emulation lands.
