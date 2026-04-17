# 051 Implement Demo PTY Terminal Session Contract

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Deepen the runner-owned demo terminal/session contract with PTY-backed
interactive semantics so demos that genuinely require terminal behavior can run
honestly without nested TUI launch.

## In Scope

- add PTY-backed execution semantics for run-backed demos that need real
  interactive terminal behavior
- keep active-session inspection, logs, receipts, and history aligned with the
  shipped attached-terminal path
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- keep browser terminal work downstream of the runner contract rather than
  inventing PTY behavior client-side

## Out Of Scope

- browser tab convergence or broader browser layout changes
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process controls
- retained-history replay as an interactive terminal
- broad runtime cancellation or desktop-client work

## Acceptance Criteria

- the runner exposes honest PTY-backed demo terminal/session semantics where
  needed
- the active-session, receipt, and history surfaces remain coherent
- browser and later clients can consume the richer terminal contract without
  inventing PTY behavior themselves
- one explicit ready card remains after closeout

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into browser tabs instead of runner contract work
- the design requires nested TUI launch
- the implementation drifts into generic process-manager ownership

## Next Task

Execute [`052-decide-demo-post-pty-terminal-contract-boundary.md`](./052-decide-demo-post-pty-terminal-contract-boundary.md)
to choose the next bounded follow-up after PTY-backed demo terminal/session
semantics landed.
