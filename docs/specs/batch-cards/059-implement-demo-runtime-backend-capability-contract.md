# 059 Implement Demo Runtime Backend Capability Contract

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Deepen the runner-owned active demo session contract so demos backed by richer
runtimes can expose honest backend/capability facts without launching nested
TUIs inside `effigy demo browser`.

## In Scope

- add bounded backend/capability metadata to the active demo session surface
- distinguish simple run-backed sessions from richer runtime-backed sessions
  without widening into generic process-manager UI
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- keep browser and text surfaces consuming runner-owned facts instead of
  inferring backend shape locally
- update help/tests/docs for the new contract

## Out Of Scope

- browser terminal input or another browser layout/control pass
- embedding the concurrent TUI inside `effigy demo browser`
- generic multi-process tab UI
- desktop-client work

## Acceptance Criteria

- active demo session payloads report bounded backend/capability facts
- the contract stays demo-scoped rather than process-manager-scoped
- browser/text consumers can describe richer runtime posture without nested TUI
- tests cover the new contract shape

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch starts inventing multi-process browser UI
- the contract requires nested TUI launch to stay coherent
- implementation drifts into generic runtime management instead of demo-scoped
  capability reporting

## Next Task

Implement this batch, then leave one explicit boundary card for what follows
after richer runtime backend capability reporting lands.
