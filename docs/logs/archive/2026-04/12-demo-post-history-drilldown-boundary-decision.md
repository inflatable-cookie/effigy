# Demo Post-History-Drilldown Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.32`

## Summary

Chose bounded history-query narrowing and selection ergonomics as the next
slice after shipped historical-attempt drilldown.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `Effigy can inspect one retained historical result cleanly but
  still relies on manual scanning and long attempt-id copy/paste for common
  history review` to `the next bounded history slice is query-first narrowing
  and human-friendly one-demo selection ergonomics`
- Remaining open:
  - implement bounded history-query narrowing controls
  - add a human-friendly retained-attempt selection path
  - keep browser/list history density deferred until the query contract is more
    settled

## Decision

- do not widen `demo list` next; discovery should stay compact and inventory-
  oriented
- do not widen the browser next; the browser baseline is stable and should not
  absorb more history density before the query contract settles further
- do deepen the dedicated `demo history` surface next, so operators can narrow
  retained results and select one historical attempt without depending only on
  long stable attempt ids

## Validation

- `git diff --check`
- `cargo run --bin effigy -- qa:docs`
- `cargo run --bin effigy -- demo history browser-proof-report`
- `cargo run --bin effigy -- demo history lifecycle-window`

## Outcome

The history lane stays one-demo and query-first. Effigy now has a clear next
batch that improves real result-review ergonomics without reopening browser
churn or widening into generic timeline tooling.

## Next Task

Use the next `g02.003` ready card to implement bounded history-query narrowing
and selection ergonomics inside `demo history`, then reassess whether any
later history density belongs in the browser or should remain query-first.
