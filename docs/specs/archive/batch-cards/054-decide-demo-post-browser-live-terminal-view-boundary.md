# 054 Decide Demo Post-Browser-Live-Terminal-View Boundary

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Choose the next bounded slice after live browser terminal consumption lands
without widening into tabs, nested TUI embedding, or generic runtime churn.

## In Scope

- assess whether the next terminal-related value belongs in:
  - bounded browser-side interaction on top of the shipped live terminal view
  - demo-scoped tab convergence such as `Overview`, `History`, `Terminal`, and
    `Artifacts`
  - another narrowly bounded runner/browser contract follow-up
- preserve the no-nested-TUI rule for demos backed by the concurrent runner
- leave the lane with one explicit ready card

## Out Of Scope

- implementing browser input, tabs, or broader runtime controls in this
  decision batch
- embedding the concurrent TUI inside `effigy demo browser`
- multi-process demo sub-tabs or generic managed-process UI
- retained-history replay as an interactive terminal
- broad runtime cancellation or desktop-client work

## Acceptance Criteria

- the next demo terminal/browser slice is explicit and bounded
- the decision keeps demo-browser terminal work demo-scoped rather than
  process-manager-scoped
- the lane remains anchored in one active ready card

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Stop Conditions

- the batch starts implementing tabs or browser input instead of deciding
- the decision requires nested TUI launch to stay coherent
- the next move becomes materially ambiguous without fresh evidence

## Decision

- do not prioritize browser-side terminal input next; the human-first path for
  interactive demos is already direct attached terminal execution, and browser
  input would reopen transport/ownership questions too early
- do not deepen runner-owned terminal semantics again immediately; the runner
  contract is already sufficient for the next browser-facing slice
- do prioritize demo-scoped tab convergence next so the browser can present the
  now-real `Overview`, `History`, `Terminal`, and `Artifacts` facets as first-
  class sibling views of one selected demo
- preserve the no-nested-TUI rule for demos backed by the concurrent runner;
  tab convergence must stay demo-scoped and must not drift into managed-process
  UI

## Next Task

Execute [`055-implement-demo-browser-demo-scoped-tabs.md`](./055-implement-demo-browser-demo-scoped-tabs.md)
to converge the browser detail surface into bounded demo-scoped tabs for
`Overview`, `History`, `Terminal`, and `Artifacts`.
