## 2026-03-11 14:00:00 - Release execute inspection drill-down

Batch: release-execute-inspection-drilldown

Context:
- Interactive `effigy release execute` already had staged review prompts.
- Operators still lacked focused drill-down for stale-state warnings and
  working-tree mismatches, especially when execute was blocked before final
  approval.

Changes:
- Added stale-warning inspection during interactive execute Step 0.
- Added working-tree item inspection during interactive execute Step 2.
- Added blocked-preflight inspection so stale/working-tree issues can be
  reviewed in detail before `release execute` returns a failure.
- Reused a shared indexed inspection parser so prepare and execute review loops
  stay consistent.
- Updated help and release docs to document the new execute drill-down flow.

Verification:
- `cargo test --lib parse_indexed_review_inspection_request_accepts_short_form -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_interactive_ -- --nocapture`

Outcome:
- Movement: baseline `interactive execute had summary-only review and blocked
  preflights returned immediately` -> current `interactive execute now supports
  focused inspection of stale and working-tree issues before approval or
  blocked-return`
