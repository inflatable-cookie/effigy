# Demo Post-Browser-Live-Attached-Terminal-Session Boundary Decision

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `071`

## Summary

The next slice is not more browser chrome and not a pause. The next bounded
move is backend parity: browser-owned live attached terminal sessions for
browser-launched single-process concurrent-runner-backed interactive demos.

## Decision

- keep the browser-owned live attached terminal session model
- broaden it one step for backend parity
- limit that expansion to single-process concurrent-runner-backed demos
- keep multi-process concurrent-runner demos on the flattened projected session
  path
- preserve the no-nested-TUI rule

## Why

- the run-backed path is now honest
- the biggest remaining mismatch is that concurrent-runner demos still fall
  back to projection even when their runtime shape is simple enough to fit the
  same live browser terminal contract
- more tab/layout churn would not close that product gap
- widening directly to multi-process live embedding would turn the browser into
  a second process manager

## Vision Target Delta

- Tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `browser-owned live attached sessions only for run-backed demos`
  to `browser-owned live attached sessions chosen as the next backend-parity
  target for bounded single-process concurrent-runner demos`
- Remaining open:
  - implement the bounded concurrent-runner live-attached browser path
  - keep multi-process concurrent-runner demos on the projected path until a
    later boundary reopens that question
