# 049 Implement Demo Attached Terminal Run Mode

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Make direct attached terminal sessions the default human interaction path for
demos that need live terminal IO, while keeping the shipped `demo input`
surface as secondary automation/client infrastructure.

## In Scope

- add a human-facing attached terminal run mode for demos whose runtime needs
  live terminal interaction
- keep receipts, latest-attempt state, and history semantics aligned with the
  shipped demo runner contract
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- keep `demo input` available as secondary machine/client infrastructure rather
  than the primary human UX

## Out Of Scope

- browser tab convergence or broader browser layout redesign
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process ownership
- retained-history replay as an interactive terminal
- broad runtime cancellation or desktop-client work

## Acceptance Criteria

- human-launched demos that need terminal IO can attach directly to a live
  terminal session
- the attached path preserves honest runner-owned receipts and active-session
  state
- `demo input` remains available but is no longer the primary human interaction
  path
- one explicit ready card remains after closeout

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into browser tabs instead of attached terminal execution
- the design requires nested TUI launch
- the implementation breaks receipt/history semantics to make attached mode work

## Next Task

Execute [`050-decide-demo-post-attached-terminal-run-boundary.md`](./050-decide-demo-post-attached-terminal-run-boundary.md)
to choose the next bounded follow-up after attached terminal run mode lands.
