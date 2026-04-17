# 037 Implement Demo History Attempt Drilldown

Status: complete
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Deepen the dedicated `demo history` query surface so operators can select one
retained attempt and inspect its result details without widening `demo list` or
the browser.

## In Scope

- expose stable attempt identifiers clearly in `demo history`
- add a bounded one-attempt drilldown mode to the history query surface
- render the selected historical attempt's outcome, summary, receipt, artifact,
  and log references in text and JSON
- prove the drilldown flow against the self-hosted demos and retained attempt
  history

## Out Of Scope

- browser-side history rendering or timeline panes
- `demo list` density changes or history badges
- multi-demo history aggregation, queueing, or generic analytics
- broader runtime cancellation or desktop-client work

## Acceptance Criteria

- operators can select a retained historical attempt from the dedicated history
  surface using a stable identifier
- the selected attempt can be inspected without relying on filesystem hunting
  or browser-specific rendering
- `demo history` remains a one-demo runner/query surface rather than turning
  into generic timeline tooling

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`
- `git diff --check`

## Stop Conditions

- the batch widens into browser history UI instead of runner/query behavior
- the drilldown contract requires multi-demo aggregation to feel coherent
- the implementation breaks the existing `demo history <id>` summary path
  instead of extending it

## Next Task

Execute [`038-decide-demo-post-history-drilldown-boundary.md`](./038-decide-demo-post-history-drilldown-boundary.md)
to choose the next bounded follow-up after historical-attempt drilldown without
reopening browser churn or widening into generic timeline tooling.
