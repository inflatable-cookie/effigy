# Demo Browser State And Query Polish Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.11`

## Summary

Shipped the browser-facing demo query/state polish slice on top of the existing
registry, inspection, run, and lifecycle-control foundation.

Delivered in this batch:

- focused `effigy demo list` filters for text, owner, tag, mode, cover,
  status, gap, and stale state
- bounded `effigy demo list --group-by ...` support for owner, tag, mode,
  cover, status, and gap
- grouped and query-aware JSON/text list output
- explicit browser-facing action availability in `demo list` and
  `demo inspect`
- explicit receipt presence and freshness in latest-attempt inspection payloads

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `demo lifecycle control without browser-shaped query/state output`
  to `runner-owned proof browsing surfaces that a future TUI can consume
  directly`
- Remaining open:
  - first bounded browser/TUI implementation slice
  - broader stoppability/runtime expansion beyond runner-owned attempts
  - later consumer-repo adoption of the demo surface

## Validation

- `cargo test --test cli_output_tests cli_demo_list_json_filters_and_groups_browser_state`
- `cargo test --test cli_output_tests cli_demo_help_is_command_specific`
- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Outcome

The demo runner now exposes the minimum honest query/state layer needed for a
browser: focused discovery, grouping, explicit gap/freshness state, and action
availability without pretending the CLI itself is already the browser.

## Next Task

Use the next `g02.003` ready card to decide the first bounded browser/TUI
foundation slice on top of the now-shipped query/state surface.
