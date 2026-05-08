# 055 Implement Demo Browser Demo-Scoped Tabs

Status: archived
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Converge the browser detail surface into bounded demo-scoped tabs so one
selected demo can switch cleanly between `Overview`, `History`, `Terminal`, and
`Artifacts` without drifting into nested TUI or managed-process UI.

## In Scope

- introduce bounded demo-scoped tabs for the selected demo detail surface
- map existing one-demo browser content into `Overview`, `History`, `Terminal`,
  and `Artifacts` views
- keep history and terminal views consuming the shipped runner-owned contracts
- preserve the no-nested-TUI rule for demos backed by the concurrent runner

## Out Of Scope

- browser text input or broader interactive terminal controls
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process UI
- retained-history replay as an interactive terminal
- broad runtime cancellation or desktop-client work

## Acceptance Criteria

- the selected demo detail surface has explicit demo-scoped tabs
- tabs remain views of one selected demo, not a process-manager model
- history and terminal tabs keep consuming runner-owned contracts without
  inventing new session semantics
- one explicit ready card remains after closeout

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into browser input or nested TUI embedding
- tab semantics drift into managed-process ownership
- the implementation requires new runner contract work to feel coherent

## Next Task

Execute [`056-decide-demo-post-browser-tab-convergence-boundary.md`](./056-decide-demo-post-browser-tab-convergence-boundary.md)
to choose the next bounded follow-up after demo-scoped tab convergence lands.
