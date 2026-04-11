# Demo History Query Follow-Up Boundary Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.30`

## Summary

Chose historical-attempt drilldown inside the dedicated `demo history` surface
as the next bounded follow-up after the shipped history query foundation.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `Effigy can list one demo's retained recent results but cannot yet
  inspect one prior result cleanly` to `the next bounded history slice is
  stable attempt selection plus one-attempt drilldown inside the dedicated
  history surface`
- Remaining open:
  - implement stable attempt selection and one-attempt drilldown in
    `demo history`
  - keep browser/list history density deferred until the drilldown contract is
    proven useful
  - keep broader runtime expansion separate from history review

## Decision

- do not widen `demo list` next; discovery should stay compact and
  inventory-oriented
- do not widen the browser next; it just stabilized around a lower-noise
  baseline and should not become the place where history semantics are invented
- do deepen the dedicated `demo history` surface next, so operators can select
  one retained attempt and inspect its receipt, artifact, and log references
  directly

## Validation

- `git diff --check`
- `effigy qa:docs`

## Outcome

The history lane stays runner-first and one-demo scoped. Effigy now has a
clear next batch that answers a real operator question without widening into
browser churn or generic timeline tooling.

## Next Task

Use the next `g02.003` ready card to implement bounded historical-attempt
drilldown inside `demo history`, then reassess whether any later history
density belongs in the browser or should remain query-first.
