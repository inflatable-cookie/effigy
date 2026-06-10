# Demo History Surface Follow-Up Boundary Decision

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.28`

## Summary

Chose a separate result-history query surface as the next bounded follow-up
after runner-side attempt history.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `Effigy has bounded retained attempt history but no clear next
  place to surface it` to `the next bounded history slice is a dedicated query
  surface rather than more density in demo list or the browser`
- Remaining open:
  - implement the dedicated history query surface
  - keep browser/list history rendering deferred until the query contract is
    proven useful
  - keep broader runtime expansion separate from history visibility

## Decision

- do not widen `demo list` next; discovery should stay compact and inventory-
  oriented
- do not widen the browser next; it only just settled into a calmer density
  model and forcing history there now would recreate presentation churn
- do implement a separate result-history query surface for one demo next, so
  operators can review retained attempt history without overloading the shipped
  list/browser surfaces

## Validation

- `git diff --check`
- `effigy qa:docs`

## Outcome

The history contract now has a clean next home. Effigy can prove the utility
of result-history queries through a dedicated CLI surface before committing that
history density into either list output or browser layout.

## Next Task

Use the next `g02.003` ready card to implement the separate demo-history query
surface on top of the shipped runner-side attempt history.
