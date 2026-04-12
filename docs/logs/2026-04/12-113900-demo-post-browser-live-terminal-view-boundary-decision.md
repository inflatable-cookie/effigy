# Demo Post-Browser-Live-Terminal-View Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`054-decide-demo-post-browser-live-terminal-view-boundary.md`](../../specs/batch-cards/054-decide-demo-post-browser-live-terminal-view-boundary.md)

## Summary

Chose bounded demo-scoped tab convergence as the next slice after live browser
terminal consumption landed.

## Vision Target Delta

- move from `browser has real one-demo history and terminal views but they are
  still mode-switched detail panes` toward `browser presents those one-demo
  facets as explicit sibling views`
- keep demo-browser organization demo-scoped instead of process-manager-shaped
- remaining gap: implement tabs without widening into browser input or nested
  TUI

## Decision

- do not prioritize browser terminal input next
- do not prioritize another runner-only contract batch next
- do prioritize bounded demo-scoped tabs for `Overview`, `History`,
  `Terminal`, and `Artifacts`
- preserve the no-nested-TUI rule for demos backed by the concurrent runner

## Why

- the browser now has enough settled one-demo surfaces for tab convergence to
  add real value instead of presentation churn
- direct attached terminal runs already cover the honest human interactive path
- browser input still reopens transport questions the lane does not need to
  answer yet

## Outcome

Opened ready card [`055-implement-demo-browser-demo-scoped-tabs.md`](../../specs/batch-cards/055-implement-demo-browser-demo-scoped-tabs.md).

## Next Task

Execute [`055-implement-demo-browser-demo-scoped-tabs.md`](../../specs/batch-cards/055-implement-demo-browser-demo-scoped-tabs.md)
to converge the browser detail surface into bounded demo-scoped tabs for
`Overview`, `History`, `Terminal`, and `Artifacts`.
