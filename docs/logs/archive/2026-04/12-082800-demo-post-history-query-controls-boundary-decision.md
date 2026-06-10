# Demo Post-History-Query-Controls Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.34`

## Summary

Chose a narrow browser-consumption handoff as the next slice after shipped
one-demo history query controls.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `Effigy has a settled one-demo history query contract but no
  decision yet on whether later history density should stay query-first or
  move into a client consumer` to `the next bounded slice can move into the
  browser only as a narrow handoff to the dedicated history surface, while
  list/browser density still stays deferred`
- Remaining open:
  - implement the bounded browser history handoff
  - keep retained history tables/timelines out of the browser until there is
    stronger evidence for denser client rendering
  - keep multi-demo history and generic analytics deferred

## Decision

- do not widen `demo list` next; history density there would still overload an
  inventory-oriented discovery surface
- do not add browser-side retained tables, badges, or timelines next; the
  self-hosted demos still do not justify another density pass
- do let the next bounded value move into a client/browser consumer, but only
  as a one-demo history handoff that consumes the settled `demo history`
  runner contract instead of inventing browser-local semantics

## Validation

- `git diff --check`
- `cargo run --bin effigy -- qa:docs`
- `cargo run --bin effigy -- demo history browser-proof-report`
- `cargo run --bin effigy -- demo history lifecycle-window`

## Outcome

The history lane stays disciplined: runner-owned query semantics remain the
source of truth, while the next slice finally allows a narrow browser
consumption step without reopening list density or generic timeline work.

## Next Task

Execute [`041-implement-demo-browser-history-handoff.md`](../../../specs/batch-cards/041-implement-demo-browser-history-handoff.md)
to let the browser consume the settled one-demo history contract through a
bounded handoff without adding list density or in-browser timelines.
