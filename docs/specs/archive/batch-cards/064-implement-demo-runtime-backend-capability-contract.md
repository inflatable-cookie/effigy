# 064 Implement Demo Runtime Backend Capability Contract

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add bounded runner-owned runtime backend and capability facts to active demo
session surfaces so richer runtimes can project honest demo-scoped behavior
without nested TUI launch or browser-invented semantics.

## In Scope

- extend active demo session and inspect payloads with bounded backend identity
  and capability facts
- distinguish the current task-backed, run-backed, and future richer runtime
  paths through one demo-scoped contract
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- keep the contract useful to both text/json CLI surfaces and the browser

## Out Of Scope

- implementing a new richer runtime backend
- browser layout or control changes
- generic process-manager UI or multi-process sub-tabs
- desktop-client work

## Acceptance Criteria

- active demo session payloads report bounded backend/capability facts
- the contract stays demo-scoped rather than process-manager-scoped
- concurrent-runner-backed demos remain flattened behind the demo session
  contract instead of requiring nested TUI launch

## Result

- `demo inspect` now reports bounded `runtime_backend` identity and capability
  facts at the demo, active-attempt, and active-terminal-session layers
- current task-backed and run-backed demos now project honest backend labels
  and capability sets through one demo-scoped contract
- active-attempt legacy records infer backend identity safely, so the contract
  stays backward-compatible while preserving the no-nested-TUI rule for future
  richer runtimes

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch drifts into implementing a richer runtime backend
- the contract requires nested TUI launch to stay coherent
- the shape becomes generic process-manager metadata instead of demo-scoped
  capability reporting

## Next Task

Execute [`065-decide-demo-post-runtime-backend-capability-boundary.md`](./065-decide-demo-post-runtime-backend-capability-boundary.md)
to choose the next bounded slice after backend/capability reporting landed
without reopening browser churn or widening into generic runtime-manager work.
