# 068 Implement Demo Concurrent-Runner Terminal Interaction Projection

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Project bounded terminal interaction for concurrent-runner-backed demos
through the existing demo session contract so browser and CLI consumers can
use input and resize semantics without nested TUI launch.

## In Scope

- add one bounded input-forwarding projection for concurrent-runner-backed
  active demo sessions
- add one bounded resize projection for concurrent-runner-backed active demo
  sessions
- keep the interaction shape demo-scoped and flattened rather than exposing a
  generic process-manager surface
- preserve receipts, logs, history, and the no-nested-TUI rule

## Out Of Scope

- browser layout or control redesign
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process controls
- broader concurrent-runner UX outside the demo contract
- desktop-client work

## Acceptance Criteria

- concurrent-runner-backed active demo sessions report honest input and resize
  availability through the demo session contract
- browser and CLI consumers can use the projected interaction path without
  nested TUI launch
- the contract stays demo-scoped rather than process-manager-scoped

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch requires launching a nested TUI to stay coherent
- the slice widens into generic process-manager interaction controls
- the design cannot stay flattened at the one-demo session boundary

## Next Task

Execute this card to add bounded concurrent-runner terminal interaction
projection through the demo session contract.

