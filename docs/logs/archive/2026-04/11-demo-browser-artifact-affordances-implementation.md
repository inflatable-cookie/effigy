# Demo Browser Artifact Affordances Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.16`

## Summary

Shipped bounded artifact-opening support inside the demo browser.

Delivered in this batch:

- artifact selection inside `effigy demo browser`
- in-browser open action for the selected artifact path
- honest failure reporting for missing artifact targets or unavailable opener
  commands
- updated browser/operator docs for the new artifact affordance

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `browser can display artifact paths but cannot act on them` to
  `browser can surface runner-owned artifact references as usable operator
  affordances`
- Remaining open:
  - decide whether live log visibility is now the next honest browser slice
  - keep broader runtime cancellation out of the browser lane
  - keep desktop-client questions deferred

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Outcome

The browser now closes the most obvious proof-inspection gap exposed by the
self-hosted demos: recorded artifacts are no longer inert strings in the detail
pane. Operators can move across artifact references and open the selected one
without dropping back to ad hoc shell work.

## Next Task

Use the next `g02.003` ready card to decide whether live log visibility is the
next honest browser follow-up.
