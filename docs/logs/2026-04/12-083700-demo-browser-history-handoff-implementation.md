# Demo Browser History Handoff Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.35`

## Summary

Implemented a bounded browser history handoff so the browser can consume the
settled one-demo `demo history` contract without adding retained-history
tables or timelines of its own.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `Effigy has a settled one-demo history query contract but the
  browser has no direct handoff into that dedicated surface` to `the browser
  now exposes one clear history handoff for the selected demo while the
  dedicated runner-owned history surface remains the source of truth`
- Remaining open:
  - decide whether any later history/browser follow-up should deepen browser
    consumption further or return to query-first runner work
  - keep browser-side retained tables/timelines deferred until there is
    stronger evidence for denser client rendering
  - keep multi-demo history and generic analytics deferred

## Delivered

- added an `Open history` browser action for the selected demo
- added a compact `History` block in the browser detail pane showing the exact
  `effigy demo history <DEMO_ID>` handoff command
- made the browser leave the TUI and run the dedicated `demo history` surface
  in normal terminal mode instead of inventing browser-local retained-history
  semantics
- updated help, changelog, roadmap/currentness surfaces, and the active ready
  card state for the next strict-lane decision batch

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

The browser can now participate in history review without becoming a second
history UI. Operators stay anchored on the settled one-demo `demo history`
contract, and later client work can still decide deliberately whether denser
browser history belongs anywhere at all.

## Next Task

Execute [`042-implement-demo-browser-integrated-history-view.md`](../../specs/batch-cards/042-implement-demo-browser-integrated-history-view.md)
to replace the shipped browser history handoff with an integrated retained-
history view inside the detail pane.
