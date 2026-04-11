# 030 Implement Demo Browser Metadata Query Parity

Status: ready
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Extend `effigy demo browser` so operators can filter and group by the already
declared demo metadata that the CLI query contract and self-hosted demos now
expose: `tag`, `mode`, `cover`, and the fuller `group-by` set.

## In Scope

- add bounded in-browser controls for `tag`, `mode`, and `cover` filters
- extend in-browser grouping controls so the browser can cycle across the full
  shipped `group-by` contract, not just the current subset
- keep query summaries and empty-state messaging honest as these dimensions are
  added
- prove the slice against the shipped self-hosted demos, which already declare
  distinct `mode`, `covers`, and `tags`

## Out Of Scope

- richer rendering or artifact preview
- deeper runtime cancellation or process orchestration work
- multi-attempt history
- desktop-client foundation work

## Acceptance Criteria

- the browser can narrow demos by `tag`, `mode`, and `cover` without dropping
  back to `demo list`
- the browser can group using the full shipped grouping contract
- the slice stays bounded to query parity and does not widen into unrelated
  browser or runtime polish

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch starts inventing new query semantics instead of reusing the shipped
  `demo list` contract
- the batch widens into richer rendering, terminal behavior, or runtime control
- the batch depends on more demos or multi-attempt history to feel coherent

## Next Task

Ship bounded metadata-query parity in the browser, then reassess whether the
next browser gap is richer detail affordances or another query/display
boundary.
