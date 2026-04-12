# 061 Decide Demo Post-Browser-Terminal-Emulator Boundary

Status: complete
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

## Decision

- do not start another browser-layout or browser-control follow-up immediately
- do not widen into generic runtime-manager work
- do deepen runner-owned terminal/session fidelity next
- make the next slice terminal size and resize handoff for active demo
  sessions, so terminal-aware demos can react honestly without nested TUI
  embedding

## Why

- embedded browser terminal emulation is now real enough that more browser
  chrome work would be churn
- the next honest gap is not presentation but fidelity: terminal-aware demos
  still need runner-owned size semantics rather than a fixed replay surface
- size/resize stays demo-scoped, strengthens both attached and browser
  terminal paths, and avoids importing the concurrent TUI app model wholesale

## Next Task

Execute [`062-implement-demo-active-terminal-resize-contract.md`](./062-implement-demo-active-terminal-resize-contract.md)
to add runner-owned terminal size and resize handoff for active demo sessions
without reopening browser churn or nested TUI embedding.
