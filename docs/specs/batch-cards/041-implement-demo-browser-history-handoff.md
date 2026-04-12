# 041 Implement Demo Browser History Handoff

Status: complete
Updated: 2026-04-12
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Let the browser consume the settled one-demo history contract through a bounded
handoff without adding list density or in-browser timelines.

## In Scope

- add a narrow one-demo browser affordance that points operators from the
  selected demo into its dedicated history surface
- keep the affordance anchored on the shipped `demo history` query contract
  rather than inventing browser-local history semantics
- preserve the existing compact browser list/detail posture while proving that
  a client can now consume the settled history contract safely

## Out Of Scope

- browser-side retained history tables, panes, badges, or timelines
- `demo list` history summaries or grouping changes
- multi-demo history aggregation, analytics, queueing, or broader runtime work

## Acceptance Criteria

- the browser exposes one clear history handoff for the selected demo without
  widening list density
- the handoff consumes the existing one-demo history contract instead of
  creating new browser-owned history semantics
- the lane stays bounded around browser consumption rather than generic UI
  churn

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Stop Conditions

- the batch starts inventing browser-local history panes or timeline semantics
- the browser change depends on multi-demo history density to feel coherent
- the handoff cannot stay aligned with the shipped `demo history` query
  contract

## Next Task

Execute [`042-decide-demo-post-browser-history-handoff-boundary.md`](./042-decide-demo-post-browser-history-handoff-boundary.md)
to decide whether any later history/browser follow-up should deepen browser
consumption further or return to query-first runner work.
