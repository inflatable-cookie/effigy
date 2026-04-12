# Demo Concurrent Runtime Projection-Shape Contract

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `074`

## Summary

Concurrent-runner-backed demos now project explicit runtime shape through the
demo contract. Clients can tell when one demo still fits one live terminal and
when multiple managed processes force the demo to stay on a flattened
projected path.

## Changes

- added runner-owned `projection_shape` reporting under `runtime_backend` for:
  - demo detail
  - active attempt
  - active terminal/session
- the shape now reports:
  - `kind`
  - `live_terminal_eligible`
  - `projected_multi_process`
  - `managed_process_count`
- persisted active-attempt records now carry projection-shape truth, with
  legacy fallback inference kept for older records
- taught the browser to prefer live attach from explicit projection-shape truth
  instead of inferring through backend kind alone
- added regression coverage for:
  - inactive single-process concurrent-runner shape
  - inactive multi-process projected shape
  - active single-process projection shape
  - active multi-process projection shape

## Vision Target Delta

- Tags: `CONTRACT`, `OPERATE`, `ROUTE`
- Moved from `clients infer concurrent demo shape from backend/capability hints`
  to `runner-owned explicit projection-shape truth across inspect and active
  terminal/session payloads`
- Remaining open:
  - decide whether the next slice should deepen concurrent-runtime truth again,
    add one bounded browser follow-up that consumes the richer shape, or pause
    this branch
