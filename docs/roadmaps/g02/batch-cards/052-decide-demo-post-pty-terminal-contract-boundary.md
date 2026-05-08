# 052 Decide Demo Post-PTY-Terminal-Contract Boundary

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after PTY-backed demo terminal/session semantics
land without widening into nested TUI embedding or generic runtime churn.

## In Scope

- assess whether the next terminal-related value belongs in:
  - deeper runner-owned input/session semantics on top of the shipped PTY path
  - bounded browser terminal convergence on top of the richer runner contract
  - demo-scoped tab convergence such as `Overview`, `History`, `Terminal`, and
    `Artifacts`
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- leave the lane with one explicit ready card

## Out Of Scope

- implementing browser tabs, browser input, or broader runtime controls in
  this decision batch
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process UI
- retained-history replay as an interactive terminal
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- the next terminal/browser slice is explicit and bounded
- the decision keeps demo-browser terminal work demo-scoped rather than
  process-manager-scoped
- the lane remains anchored in one active ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing tabs or browser input instead of deciding
- the decision requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh evidence

## Decision

- do not deepen runner-owned terminal semantics again immediately; attached
  terminal runs, input-contract plumbing, and PTY-backed session reporting now
  form a sufficient runner baseline for the next consumer slice
- do not jump to demo-scoped tabs yet; tabs are still presentation convergence
  and would mix too many browser changes into one batch
- do prioritize bounded browser terminal convergence next so the browser can
  consume the richer active-session contract as a live demo-scoped terminal
  surface rather than a static recent-lines summary
- preserve the no-nested-TUI rule for demos backed by the concurrent runner;
  the browser should render the demo session itself, not launch the concurrent
  TUI inside the browser

## Next Task

Execute [`053-implement-demo-browser-live-terminal-view.md`](./053-implement-demo-browser-live-terminal-view.md)
to let `effigy demo browser` consume the shipped demo terminal/session contract
as a bounded live terminal view before any tab convergence work.
