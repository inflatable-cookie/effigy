# Demo Browser Live Concurrent-Runner Session Parity

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `072`

## Summary

Browser-owned live attached terminal sessions now cover the bounded
single-process concurrent-runner path as well as the earlier run-backed path.
Multi-process concurrent-runner demos stay on the projected session surface.

## Changes

- added one bounded browser-live-attach capability fact to demo runtime backend
  reporting
- taught the browser to use the live attached terminal path when that
  capability is present instead of hard-coding run-backed-only selection
- enabled attached stdin handoff for single-process concurrent-runner text runs
  so browser-launched live sessions can interact honestly through the existing
  demo-owned input path
- added regression coverage for:
  - inactive single-process concurrent-runner live-attach eligibility
  - inactive multi-process fallback behavior
  - active concurrent-runner capability projection
  - attached text-run input handoff for the bounded single-process case

## Vision Target Delta

- Tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `browser live attached sessions only on run-backed demos` to
  `browser live attached sessions on run-backed demos plus bounded
  single-process concurrent-runner parity`
- Remaining open:
  - decide whether the next slice should deepen concurrent-runtime/browser
    parity further or pause browser-terminal work again
