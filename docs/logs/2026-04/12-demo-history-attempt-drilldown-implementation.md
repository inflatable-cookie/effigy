# Demo History Attempt Drilldown Implementation

Date: 2026-04-12
Roadmap: `g02.003`
Batch: `03.31`

## Summary

Implemented bounded historical-attempt drilldown inside `effigy demo history`
so operators can select one retained attempt by stable id and inspect its
receipt, artifact, and log references directly.

## Vision Target Delta

- Primary tags: `OPERATE`, `CONTRACT`, `ROUTE`
- Moved from `Effigy can list one demo's retained recent results but not
  inspect one prior result cleanly` to `Effigy now supports one-demo history
  summary plus stable selected-attempt drilldown in text and JSON`
- Remaining open:
  - decide whether any later history density belongs in `demo list`, the
    browser, or a deeper query contract
  - keep multi-demo history and generic analytics deferred
  - keep broader runtime expansion separate from history review

## Delivered

- added `--attempt <ATTEMPT_ID>` to `effigy demo history <DEMO_ID>`
- exposed stable attempt ids directly in the visible recent-attempt history
  table
- added `selected_attempt` to the JSON result payload for drilldown queries
- added a bounded text drilldown section for one retained attempt covering:
  - outcome
  - summary
  - receipt reference
  - artifact references
  - stdout/stderr log references
- updated help, README, command-reference, manifest-cookbook, and changelog
  surfaces

## Validation

- `cargo test`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa`
- `git diff --check`
- live self-hosted drilldown check:
  - `cargo run --bin effigy -- demo history browser-proof-report --attempt browser-proof-report-1775944053944 --json`

## Outcome

The demo-history contract now answers both:

- what recent retained results exist for one demo
- show me one prior retained result properly

That keeps result review query-first and gives later UI work a stable
historical-attempt contract to consume instead of inventing it through
presentation.

## Next Task

Use the next `g02.003` ready card to choose the next bounded follow-up after
historical-attempt drilldown without widening into browser churn or generic
timeline tooling.
