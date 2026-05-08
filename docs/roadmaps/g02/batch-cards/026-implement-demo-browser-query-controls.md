# 026 Implement Demo Browser Query Controls

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Add bounded in-browser query controls to `effigy demo browser` so an operator
can narrow the registry without dropping back to `demo list`.

## In Scope

- expose a bounded query/edit surface inside the browser
- reuse the shipped demo-list query model instead of inventing browser-only
  semantics
- support narrowing by the highest-signal existing dimensions such as search,
  owner, status, gap, and stale state
- keep grouping and detail browsing coherent while filters are active

## Out Of Scope

- new demo query semantics beyond the shipped CLI contract
- richer log streaming or terminal emulation
- artifact preview/rendering
- multi-attempt history
- desktop-client work

## Acceptance Criteria

- the browser can narrow the visible demo list without leaving the TUI
- the query state is visible and understandable to the operator
- the browser reuses existing demo-list filtering semantics rather than forking
  them
- empty-result states are surfaced honestly

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch turns into a general text-editor widget project
- the batch invents browser-only filter semantics that diverge from `demo list`
- the batch reopens log streaming, artifact preview, or runtime cancellation
  scope

## Next Task

Decide the next bounded browser follow-up through
[`027-decide-demo-post-query-follow-up-boundary.md`](./027-decide-demo-post-query-follow-up-boundary.md).
