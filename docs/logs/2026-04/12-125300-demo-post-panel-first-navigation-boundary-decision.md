# Demo Post-Panel-First-Navigation Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Card: [`058-decide-demo-post-panel-first-navigation-boundary.md`](../../specs/batch-cards/058-decide-demo-post-panel-first-navigation-boundary.md)

## Summary

Chose to pause browser work and return to runner-owned richer-runtime session
semantics.

## Vision Target Delta

- move from `browser structure and controls still need obvious correction`
  toward `browser is coherent enough that the next substantial gap is runner
  contract depth`
- keep the lane demo-scoped and avoid process-manager-shaped browser drift
- remaining gap: expose richer runtime/backend facts for demos without nested
  TUI launch

## Decision

- do not prioritize another browser follow-up immediately
- do return to runner/query contract work next
- do make the next slice richer-runtime backend/capability reporting for active
  demo sessions
- preserve the no-nested-TUI rule for demos backed by the concurrent runner

## Why

- panel-first navigation fixed the last obvious browser control mismatch
- the browser now consumes overview/history/terminal/artifacts through one
  coherent one-demo shape
- the next broad product risk is richer demo runtimes forcing semantics through
  browser presentation instead of exposing them from the runner

## Outcome

Opened ready card [`059-implement-demo-runtime-backend-capability-contract.md`](../../specs/batch-cards/059-implement-demo-runtime-backend-capability-contract.md).

## Next Task

Execute [`059-implement-demo-runtime-backend-capability-contract.md`](../../specs/batch-cards/059-implement-demo-runtime-backend-capability-contract.md)
to deepen the runner-owned active session contract for richer demo runtimes
without reopening browser churn or nested TUI embedding.
