# 074 Implement Demo Concurrent Runtime Projection-Shape Contract

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add runner-owned projection-shape facts for concurrent-runner-backed demos so
clients can tell when a demo is a single-terminal live-attach candidate versus
a projected multi-process runtime that should stay on the flattened session
path.

## In Scope

- add bounded demo/runtime contract facts for concurrent-runner-backed demos:
  - single-process vs multi-process projection shape
  - whether the runtime still fits one demo-owned live terminal
  - whether the session is projected because multiple managed processes are
    active behind it
- expose those facts through inspect and active terminal/session payloads
- keep browser consumers reading runner truth instead of inventing their own
  multi-process heuristics
- update roadmap/currentness/help/log surfaces in the same closeout

## Out Of Scope

- multi-process browser tabs or panes
- embedding the concurrent TUI
- generic process-manager controls
- redesigning the browser layout again
- desktop-client work

## Acceptance Criteria

- concurrent-runner demo payloads expose bounded projection-shape facts
- single-terminal and projected-multi-process cases are distinguishable from
  runner-owned data
- browser and future clients no longer need to infer multi-process shape from
  backend kind alone
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
  demo-scoped runtime shape
- implementation would require nested TUI launch to feel coherent

## Next Task

Execute
`075-decide-demo-post-concurrent-runtime-projection-shape-boundary.md` to make
the next bounded boundary call after runner-owned projection-shape truth
landed.
