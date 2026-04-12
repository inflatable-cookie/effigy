# 047 Implement Demo Active Terminal Input Contract

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Deepen the runner-owned active demo terminal/session contract with bounded
input-forwarding semantics so later browser terminal work can support live demo
interaction without nested TUIs.

## In Scope

- extend the one-demo active terminal/session contract with explicit
  input-forwarding capability and invocation shape
- keep the contract active-attempt scoped rather than turning into generic
  process-manager control
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- expose enough runner-owned surface that a later browser terminal view can
  send bounded input honestly

## Out Of Scope

- browser tab convergence or broader browser layout changes
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process controls
- retained-history replay as an interactive terminal
- broad runtime cancellation or desktop-client work

## Acceptance Criteria

- the runner exposes a bounded active-demo input contract alongside the shipped
  terminal/session view contract
- the contract stays demo-scoped and does not require nested TUI launch
- later browser work can consume the contract without inventing terminal-input
  semantics client-side
- one explicit ready card remains after closeout

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into browser tab work instead of runner contract work
- the contract requires generic multiprocess ownership to stay coherent
- the design depends on launching a nested TUI

## Next Task

Implement the next ready follow-up selected after this runner contract slice
lands.
