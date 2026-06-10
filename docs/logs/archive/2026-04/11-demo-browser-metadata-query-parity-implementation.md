# Demo Browser Metadata Query Parity Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `03.24`

## Summary

Shipped bounded metadata-query parity inside `effigy demo browser`.

Delivered in this batch:

- in-browser `tag` and `cover` prompt filters
- bounded `mode` filter cycling in the browser
- full `group-by` parity across owner, tag, mode, cover, status, and gap
- updated query summaries, empty-state messaging, and browser help/docs for
  the expanded query model

## Vision Target Delta

- Primary tags: `OPERATE`, `ROUTE`, `CONTRACT`
- Moved from `the browser can inspect and navigate selected demos but still
  leaves metadata-only filtering and grouping to demo list` to `the browser now
  reaches practical parity with the shipped metadata query contract`
- Remaining open:
  - decide the next bounded browser follow-up after metadata-query parity
  - keep richer rendering, deeper runtime work, and desktop-client questions
    deferred until that next explicit boundary choice

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Outcome

The first browser now supports the full practical discovery loop for the
shipped self-hosted demos from one surface: narrow by metadata, group by the
same dimensions the CLI already understands, inspect detail, run, stop/rerun,
open artifacts, and read bounded recent output. The next decision can focus on
what operator-visible gap remains after that fuller baseline, not on missing
query parity.

## Next Task

Use the next `g02.003` ready card to decide the next bounded browser follow-up
after metadata-query parity.
