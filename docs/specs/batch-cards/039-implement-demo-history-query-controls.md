# 039 Implement Demo History Query Controls

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Deepen the dedicated `demo history` surface with bounded query controls so
operators can narrow and select retained attempts without widening `demo list`
or the browser.

## In Scope

- add one-demo history narrowing controls such as outcome-focused filtering
- add a human-friendly retained-attempt selection path so operators do not have
  to copy long attempt ids for common drilldown flows
- keep text and JSON output aligned around the new query controls
- prove the controls against retained history behavior without requiring any
  browser rendering changes

## Out Of Scope

- browser-side history panes, badges, or timelines
- `demo list` history summaries or grouping changes
- multi-demo history aggregation, analytics, queueing, or broader runtime work

## Acceptance Criteria

- operators can narrow one demo's retained history through bounded query
  controls instead of scanning the full retained table every time
- operators can select a retained attempt through at least one human-friendly
  path in addition to the stable `--attempt <ATTEMPT_ID>` machine contract
- the resulting surface remains a one-demo query contract rather than turning
  into generic timeline tooling

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch widens into browser history UI instead of runner/query behavior
- the controls depend on multi-demo aggregation or generic analytics to feel
  coherent
- the batch breaks the existing `demo history` summary-plus-drilldown contract
  instead of extending it

## Next Task

Execute [`040-decide-demo-post-history-query-controls-boundary.md`](./040-decide-demo-post-history-query-controls-boundary.md)
to decide whether any later history density should remain query-first or can
safely move into a client without reopening browser churn.
