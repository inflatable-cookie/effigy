# Demo Attempt History Foundation Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.27`

## Summary

Shipped the first bounded runner-side demo attempt-history foundation.

Delivered in this batch:

- bounded persisted terminal-attempt history per demo under `.effigy/demo/history/`
- enriched `effigy demo inspect <id>` text and JSON output with recent attempt
  history while preserving the existing latest-attempt summary
- compact history records carrying terminal outcome, summary, timestamps,
  receipt path, artifact refs, and runner-owned stdout/stderr references
- CLI contract coverage for empty history and multi-attempt inspect flows

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `operators can see only active state plus one latest terminal
  attempt` to `operators can inspect a bounded recent history of terminal demo
  outcomes without widening into browser-local timelines yet`
- Remaining open:
  - decide whether the next history slice belongs in `demo list`, the browser,
    or a separate result-timeline query surface
  - keep multi-attempt concurrency and queueing deferred
  - keep broader runtime cancellation separate from history visibility

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Outcome

Effigy now retains bounded demo outcome history as runner-owned state instead
of collapsing every finished run into one latest receipt. That gives both CLI
and later UI clients a real result-review surface without forcing history
rendering decisions into the browser before the underlying contract settles.

## Next Task

Use the next `g02.003` ready card to decide whether demo history should widen
through `demo list`, the browser, or a separate result-timeline query surface.
