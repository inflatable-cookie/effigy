# 076 Implement Demo Concurrent Runtime Projected Process Summary Contract

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add bounded runner-owned process summary facts for concurrent-runner demos that
stay on the flattened projected path, so clients can tell what sits behind one
demo-owned terminal/session without widening into a process manager.

## In Scope

- add demo-scoped projected-runtime summary facts such as:
  - managed process names
  - managed process count parity with the projection-shape contract
  - whether the active session is merging output from multiple named managed
    processes
- expose those facts through inspect and active terminal/session payloads
- keep browser and future clients consuming runner truth instead of inventing
  projected-runtime heuristics
- update roadmap/currentness/help/log surfaces in the same closeout

## Out Of Scope

- multi-process browser panes or tabs
- embedding the concurrent TUI
- generic process-manager controls
- redesigning the browser layout
- desktop-client work

## Acceptance Criteria

- projected concurrent-runtime payloads expose bounded process summary facts
- clients can tell when one demo-owned projected terminal/session merges output
  from multiple named managed processes
- the lane closes with one new explicit ready card

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch starts building multi-process browser UI instead of deepening the
  contract
- the contract drifts into generic process-manager inventory instead of
  demo-scoped projected-runtime summary
- implementation requires nested TUI launch to feel coherent

## Next Task

Implement this runner-owned projected-runtime summary slice, then leave one new
explicit boundary card instead of widening straight into multi-process browser
controls.
