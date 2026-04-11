# Demo Browser Query Controls Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.20`

## Summary

Shipped bounded in-browser query controls inside `effigy demo browser`.

Delivered in this batch:

- browser-owned query state layered onto the shipped `demo list` contract
- one-line prompt controls for search and owner filters
- bounded cycle/toggle controls for status, gap, and stale-only filters
- visible query summary and honest empty-result handling
- updated operator docs for the browser controls

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `browser can inspect a selected demo well but still requires
  dropping back to demo list for meaningful registry narrowing` to `browser can
  browse and narrow proof inventory inside one bounded interactive surface`
- Remaining open:
  - decide the next bounded browser follow-up after query controls
  - keep richer log handling, artifact rendering, and broader runtime concerns
    deferred until the next explicit decision

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Outcome

The browser now covers the main operator loop for a growing demo registry:
discover, narrow, inspect, run, stop/rerun, open artifacts, and inspect recent
output. The next browser move should now be chosen deliberately from that more
realistic baseline instead of inferred from smaller earlier slices.

## Next Task

Use the next `g02.003` ready card to decide the next bounded browser follow-up
after query controls.
