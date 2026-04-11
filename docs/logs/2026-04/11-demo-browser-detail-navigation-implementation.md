# Demo Browser Detail Navigation Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.22`

## Summary

Shipped bounded detail-pane navigation inside `effigy demo browser`.

Delivered in this batch:

- browser-owned vertical scroll state for longer selected-demo records
- `PgUp`/`PgDn` plus `J`/`K` navigation, with `Home`/`End` jumps
- visible detail-position feedback in the detail title
- preserved artifact selection while the detail pane scrolls
- updated operator docs and browser help for the new navigation controls

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `the browser can narrow and inspect demos but long selected-demo
  records still fall off the viewport` to `the browser can reach the full
  selected-demo record without leaving the TUI`
- Remaining open:
  - decide the next bounded browser follow-up after detail navigation
  - keep richer rendering, deeper runtime work, and desktop-client questions
    deferred until the next explicit boundary choice

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Outcome

The first browser is now honest about long selected-demo records: recent output,
artifacts, receipt summaries, and proof metadata can all be reached from one
surface without forcing a second terminal or manual artifact hunting. The next
question is no longer basic navigation, but which bounded browser follow-up
closes the next operator-visible gap most cleanly.

## Next Task

Use the next `g02.003` ready card to decide the next bounded browser follow-up
after detail navigation.
