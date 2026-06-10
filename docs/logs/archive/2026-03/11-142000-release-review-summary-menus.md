## 2026-03-11 14:20:00 - Release review summary menus

Batch: release-review-summary-menus

Context:
- Interactive release prepare/execute already supported staged review and
  drill-down inspection.
- The remaining UX gap was navigation: operators still had to walk a fixed
  prompt order instead of jumping straight to the section they wanted.

Changes:
- Replaced the fixed linear interactive prepare flow with a compact review menu
  covering version review, mutation review, gate review, final preview, apply,
  and cancel.
- Replaced the fixed linear interactive execute flow with a compact review menu
  covering stale review, prepared-state review, working-tree review, final
  preview, execute, and cancel.
- Kept per-mutation and per-working-tree drill-down flows intact inside the new
  menu-driven review model.
- Updated interactive CLI tests to prove non-linear review order before apply
  and execute.
- Updated help and release docs to describe the new summary-menu contract.

Verification:
- `cargo test --test cli_output_tests cli_release_prepare_interactive_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_execute_interactive_ -- --nocapture`
- `cargo test --lib parse_indexed_review_inspection_request_accepts_short_form -- --nocapture`

Outcome:
- Movement: baseline `interactive release review required fixed linear stepping`
  -> current `interactive release review supports direct menu-based navigation
  between sections before apply/execute`
