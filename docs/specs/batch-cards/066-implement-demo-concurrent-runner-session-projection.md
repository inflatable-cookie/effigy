# 066 Implement Demo Concurrent-Runner Session Projection

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Implement one richer demo runtime backend slice by projecting
concurrent-runner-backed demos through the existing demo session contract so
they can report honest active session facts without nested TUI launch.

## In Scope

- add one bounded concurrent-runner-backed demo runtime projection behind the
  shipped demo runner surfaces
- map backend identity, active session facts, and capability reporting through
  the existing demo-scoped contract
- preserve the no-nested-TUI rule in both text/json CLI and browser consumers
- keep the implementation bounded to one demo-facing session projection shape

## Out Of Scope

- browser layout or control redesign
- generic process-manager UI or multi-process demo sub-tabs
- broad managed-runtime expansion outside the demo contract
- desktop-client work

## Acceptance Criteria

- concurrent-runner-backed demos report through the existing demo session
  contract without nested TUI launch
- runtime backend reporting stays demo-scoped rather than process-manager-
  scoped
- browser and CLI surfaces can consume the richer runtime through the same
  contract they already use

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into generic process-manager work
- the design requires launching a nested TUI to stay coherent
- the slice turns into browser-specific semantics instead of runner-owned demo
  projection

## Next Task

Execute this card to project concurrent-runner-backed demos through the demo
session contract without reopening browser churn or nested TUI embedding.
