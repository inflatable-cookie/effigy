# Demo Browser Demo-Scoped Tabs Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`055-implement-demo-browser-demo-scoped-tabs.md`](../../../specs/batch-cards/055-implement-demo-browser-demo-scoped-tabs.md)

## Summary

Shipped bounded demo-scoped tabs in `effigy demo browser`.

## Vision Target Delta

- move from `one selected demo has multiple detail modes hidden behind action
  switches` toward `one selected demo has explicit sibling views for overview,
  history, terminal, and artifacts`
- keep browser organization demo-scoped instead of process-manager-shaped
- remaining gap: decide the next bounded follow-up after tab convergence

## Delivered

- browser detail surface now exposes `Overview`, `History`, `Terminal`, and
  `Artifacts` tabs
- action-sheet duplication for history and terminal view switching is removed
- artifacts now live in their own dedicated tab instead of crowding overview
- `Esc` returns non-overview tabs to `Overview`, and `Tab`/`Shift+Tab` switch
  between demo-scoped views

## Validation

- `cargo test browser_`
- `cargo fmt --all`

## Outcome

Opened ready card [`056-decide-demo-post-browser-tab-convergence-boundary.md`](../../../specs/batch-cards/056-decide-demo-post-browser-tab-convergence-boundary.md).

## Next Task

Execute [`056-decide-demo-post-browser-tab-convergence-boundary.md`](../../../specs/batch-cards/056-decide-demo-post-browser-tab-convergence-boundary.md)
to choose the next bounded follow-up after demo-scoped browser tab convergence
landed.
