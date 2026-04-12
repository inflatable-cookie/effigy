# Demo History Query Controls Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.33`

## Summary

Implemented bounded one-demo history query controls so operators can narrow
retained attempts by outcome and select a displayed retained attempt by
ordinal in addition to the stable `--attempt <ATTEMPT_ID>` path.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `Effigy demo history can show retained attempts and drill into one
  stable attempt id, but common review still depends on manual scanning and
  long id copy/paste` to `Effigy demo history now supports outcome-focused
  narrowing plus human-friendly ordinal selection while keeping the contract
  one-demo and query-first`
- Remaining open:
  - decide whether any later history density should remain query-first or can
    safely move into a client/browser consumer
  - keep multi-demo history and generic analytics deferred
  - keep broader runtime expansion separate from history review

## Delivered

- added `--outcome <OUTCOME>` to `effigy demo history <DEMO_ID>` for retained
  `passed`, `failed`, or `terminated` filtering
- added `--ordinal <N>` to select the Nth retained attempt from the current
  narrowed history result set
- surfaced displayed ordinals directly in text and JSON history output so the
  human-friendly selector maps to visible result order
- kept the existing stable `--attempt <ATTEMPT_ID>` drilldown path intact while
  rejecting ambiguous `--attempt` plus `--ordinal` combinations
- updated help, changelog, roadmap/currentness surfaces, and the active ready
  card state for the next strict-lane decision batch

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`

## Outcome

The demo-history contract now answers three distinct one-demo review needs:

- what retained results exist
- narrow that retained set by outcome
- select one visible retained result without copying a long attempt id

That keeps history review runner-owned and query-first before any later browser
or client work decides how much of that density should be rendered directly.

## Next Task

Execute [`040-decide-demo-post-history-query-controls-boundary.md`](../../specs/batch-cards/040-decide-demo-post-history-query-controls-boundary.md)
to decide whether any later history density should remain query-first or can
safely move into a client without reopening browser churn.
