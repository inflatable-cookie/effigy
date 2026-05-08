# 033 Implement Demo Attempt History Foundation

Status: archived
Updated: 2026-04-11
Roadmap: `g02.003`
Spec: `docs/specs/archive/003-demo-harness-model-and-runner-strict-lane.md`

## Objective

Implement the first bounded runner-side demo attempt-history slice so Effigy
can retain and inspect recent terminal outcomes beyond the single latest
attempt.

## In Scope

- persist a bounded per-demo history of terminal attempts
- extend `demo inspect` text and JSON output with recent attempt history while
  keeping latest-attempt compatibility intact
- keep history records compact around timestamp/ordinal, terminal status,
  summary, and artifact/receipt references
- prove the slice against the self-hosted demo surfaces in this repo

## Out Of Scope

- browser rendering changes beyond later consumption of the richer inspect
  state
- `demo list` history summaries or timeline groupings
- multi-attempt concurrency, queueing, or generic runtime cancellation
- richer artifact preview or log streaming

## Acceptance Criteria

- terminal demo attempts are retained in a bounded per-demo history rather than
  only replacing the prior latest-attempt record
- `demo inspect` exposes that recent history in text and JSON without breaking
  the existing latest-attempt summary shape
- the implementation remains runner-side and does not widen into new browser
  work

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Stop Conditions

- the batch widens into browser history rendering instead of runner-state
  foundation
- history persistence becomes unbounded or turns into queueing/concurrency work
- the batch breaks the existing latest-attempt compatibility surface without an
  explicit contract decision

## Next Task

Execute [`034-decide-demo-history-surface-follow-up-boundary.md`](./034-decide-demo-history-surface-follow-up-boundary.md)
to choose whether the next bounded demo-history slice belongs in `demo list`,
the browser, or a separate result-timeline query surface.
