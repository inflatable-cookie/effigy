# 035 Implement Demo History Query Foundation

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Ship a separate CLI surface for querying one demo's retained attempt history
without widening `demo list` or the browser yet.

## In Scope

- add a first-class query surface for one demo's retained attempt history and
  result summaries
- keep the first delivery inspect/query focused rather than browser-facing
- prove the new history query against the self-hosted demos and retained
  terminal-attempt state

## Out Of Scope

- browser timeline rendering or browser detail expansion
- `demo list` history summaries, grouping, or density changes
- multi-attempt concurrency, queueing, or broader runtime cancellation

## Acceptance Criteria

- operators can query one demo's retained result history through a dedicated
  CLI surface instead of relying on `demo inspect` alone
- the first delivery stays runner/query-side and does not reopen browser churn
- the new surface is validated against the self-hosted demos and JSON output

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch widens into browser rendering or list density changes
- the new query surface turns into a generic timeline framework instead of a
  bounded one-demo history query
- the batch expands into runtime queueing or broader cancellation work

## Next Task

Use the next active ready card to decide where the shipped demo-history query
surface should grow next before widening list or browser rendering.
