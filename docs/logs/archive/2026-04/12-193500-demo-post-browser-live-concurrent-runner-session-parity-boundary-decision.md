# Demo Post-Browser-Live-Concurrent-Runner-Session-Parity Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `073`

## Summary

Do not take more browser churn next. Do not widen into multi-process browser
controls next. The next bounded slice is runner-owned concurrent-runtime
projection-shape truth.

## Decision

- keep the shipped browser live-terminal model as-is for now
- do not add multi-process browser tabs, panes, or nested TUI embedding
- keep the lane moving with one runner-owned contract slice
- next slice: expose bounded projection-shape facts for richer
  concurrent-runner demos

## Why

- the browser now covers the honest single-terminal cases
- the remaining gap is that richer concurrent demos still need clearer
  runner-owned truth about why they stay projected
- pushing that meaning into browser presentation would repeat earlier churn
- a projection-shape contract is the smallest slice that protects later UI
  work from inventing multi-process semantics

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Moved from `browser live-session parity landed for single-terminal cases` to
  `runner-owned projection-shape truth chosen as the next bounded slice for
  richer concurrent demos`
- Remaining open:
  - implement the bounded concurrent-runtime projection-shape contract
  - decide later whether any browser follow-up is justified once that runner
    truth is shipped
