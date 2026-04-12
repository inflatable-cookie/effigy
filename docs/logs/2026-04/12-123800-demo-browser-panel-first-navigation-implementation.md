# Demo Browser Panel-First Navigation Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`057-implement-demo-browser-panel-first-navigation.md`](../../specs/batch-cards/057-implement-demo-browser-panel-first-navigation.md)

## Summary

Shipped panel-first controls in `effigy demo browser`.

## Vision Target Delta

- move from `detail tabs own the primary keyboard switch path` toward `Tab
  switches between list and detail while arrows act inside the focused panel`
- keep the browser demo-scoped and avoid process-manager-shaped navigation
- remaining gap: decide whether browser structure is now coherent enough to
  pause or still needs one more bounded follow-up

## Delivered

- `Tab` and `Shift+Tab` now switch between the demo list and detail pane
- `←` and `→` now switch the selected detail view between `Overview`,
  `History`, `Terminal`, and `Artifacts` when the detail pane is focused
- `↑` and `↓` stay inside the focused panel, so list navigation and detail-side
  action/history/artifact navigation no longer fight each other
- browser help text and control-focused tests now match the new model

## Validation

- `cargo test browser_`
- `cargo test demo_help -- --nocapture`

## Outcome

Opened ready card [`058-decide-demo-post-panel-first-navigation-boundary.md`](../../specs/batch-cards/058-decide-demo-post-panel-first-navigation-boundary.md).

## Next Task

Execute [`058-decide-demo-post-panel-first-navigation-boundary.md`](../../specs/batch-cards/058-decide-demo-post-panel-first-navigation-boundary.md)
to choose the next bounded slice after panel-first browser navigation landed.
