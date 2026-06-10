# Demo Post-Browser-Tab-Convergence Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`056-decide-demo-post-browser-tab-convergence-boundary.md`](../../../specs/batch-cards/056-decide-demo-post-browser-tab-convergence-boundary.md)

## Summary

Chose one more bounded browser follow-up instead of refocusing away from the
browser, because operator feedback showed the converged control model is still
wrong.

## Vision Target Delta

- move from `browser still appears to need obvious missing structural work`
  toward `browser tabs exist but the control model matches the wrong primary
  navigation boundary`
- keep the lane demo-scoped and avoid drift into process-manager UI
- remaining gap: land panel-first navigation, then re-check whether browser
  work should pause

## Decision

- do not return to runner/query work yet
- do prioritize one more bounded browser interaction batch next
- do make that batch panel-first navigation:
  - `Tab` switches panels
  - left/right/up/down navigate inside the active panel
- do not prioritize browser terminal input next
- preserve the no-nested-TUI rule for demos backed by the concurrent runner

## Why

- tabs solved view structure, not control ownership
- user feedback showed the primary navigation model still fights the layout
- panel-first control is still structural browser work, not polish churn
- attached terminal execution still covers the honest human interaction path, so
  browser terminal input stays deferred

## Outcome

Opened ready card [`057-implement-demo-browser-panel-first-navigation.md`](../../../specs/batch-cards/057-implement-demo-browser-panel-first-navigation.md).

## Next Task

Execute [`057-implement-demo-browser-panel-first-navigation.md`](../../../specs/batch-cards/057-implement-demo-browser-panel-first-navigation.md)
to make `Tab` switch panels and keep left/right/up/down navigation inside the
active panel.
