# 048 Implement Demo Browser Terminal Input Affordance

Status: ready
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Let `effigy demo browser` consume the shipped runner-owned active-terminal
input contract through one bounded demo-scoped terminal interaction affordance.

## In Scope

- add one browser-side terminal input affordance for the selected demo
- consume the shipped `demo input <DEMO_ID> --text <TEXT> [--append-newline]`
  contract instead of inventing client-side transport semantics
- keep the interaction demo-scoped rather than process-manager-scoped
- reflect unavailable input-forwarding honestly when the active session reports
  it unsupported

## Out Of Scope

- browser tab convergence or broader browser layout changes
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process controls
- retained-history replay as an interactive terminal
- broad runtime cancellation or desktop-client work

## Acceptance Criteria

- the browser can trigger one bounded terminal input flow for the selected demo
- the flow stays aligned to the runner-owned demo input contract
- unsupported input-forwarding is rendered honestly
- one explicit ready card remains after closeout

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into browser tabs instead of bounded terminal interaction
- the browser starts inventing its own transport semantics
- the design requires nested TUI launch

## Next Task

Implement the next ready follow-up selected after this browser interaction
slice lands.
