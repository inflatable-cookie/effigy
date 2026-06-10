# 11 Demo History Query Foundation Implementation

Date: 2026-04-11
Roadmap: `g02.003`
Batch: `035`

## Summary

Shipped a separate `effigy demo history <DEMO_ID>` query surface so retained
terminal-attempt history no longer has to live only inside `demo inspect`.

## What Landed

- added `demo history <DEMO_ID> [--limit <N>]`
- added text and JSON output for one demo's retained attempt history
- kept `demo inspect` stable while exposing history through a dedicated query
  surface
- validated the new surface against the self-hosted demos and CLI JSON tests

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy qa`

## Vision Target Delta

- moves the demo runner from `history only visible as a secondary inspect
  detail` toward `a first-class result-history query surface that later list or
  browser work can consume without re-opening runner boundaries`

## Next Task

Use the next ready card to decide the follow-up boundary after `demo history`
before widening browser or list rendering again.
