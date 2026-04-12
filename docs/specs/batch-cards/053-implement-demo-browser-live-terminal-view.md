# 053 Implement Demo Browser Live Terminal View

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Let `effigy demo browser` consume the shipped active demo terminal/session
contract as a bounded live terminal view so operators can watch an active demo
session inside the browser without nested TUI launch.

## In Scope

- deepen the existing browser terminal view from static recent-output summary to
  bounded live session consumption for one selected demo
- keep the terminal surface demo-scoped and driven by the runner-owned
  `active_terminal_session` contract
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- keep receipts, logs, and history runner-owned; browser terminal remains a
  consumer, not a second source of truth

## Out Of Scope

- browser text input or full terminal interaction controls
- demo-scoped tabs or broader browser layout convergence
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process controls
- retained-history replay as an interactive terminal
- broad runtime cancellation or desktop-client work

## Acceptance Criteria

- the browser terminal view can follow active demo output in a bounded,
  demo-scoped way
- the browser consumes the runner-owned session contract rather than inventing
  terminal semantics client-side
- the lane remains anchored in one explicit ready card after closeout

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into browser tabs or browser input
- the implementation requires nested TUI launch
- the browser starts owning session semantics that belong to the runner

## Next Task

Execute [`054-decide-demo-post-browser-live-terminal-view-boundary.md`](./054-decide-demo-post-browser-live-terminal-view-boundary.md)
to choose the next bounded follow-up after live browser terminal consumption
lands.
