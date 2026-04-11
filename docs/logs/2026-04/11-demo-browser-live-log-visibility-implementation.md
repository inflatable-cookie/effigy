# Demo Browser Live Log Visibility Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.18`

## Summary

Shipped bounded recent-output visibility inside `effigy demo browser`.

Delivered in this batch:

- runner-owned stdout/stderr log persistence for demo attempts
- browser detail rendering for recent active-attempt or latest terminal output
- honest missing-log handling when no runner-owned output exists
- updated operator docs for the new browser-facing proof-inspection surface

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `browser can act on artifacts but still requires another terminal
  to inspect recent demo output` to `browser can surface recent runner-owned
  proof output alongside lifecycle and artifact state`
- Remaining open:
  - decide the next bounded browser follow-up after live log visibility
  - keep terminal emulation and broader runtime cancellation deferred
  - keep multi-attempt history out of the browser lane unless later evidence
    proves it is the next real gap

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Outcome

The browser now closes the next real operator gap exposed by the self-hosted
demos. `browser-proof-report` and `lifecycle-window` no longer require a second
terminal just to inspect recent proof output, while the browser still avoids
pretending it is a terminal emulator.

## Next Task

Use the next `g02.003` ready card to decide the next bounded browser follow-up
after live log visibility.
