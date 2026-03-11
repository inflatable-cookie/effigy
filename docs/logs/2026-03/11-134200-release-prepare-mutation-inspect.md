## 2026-03-11 13:42:00 - Release prepare mutation inspect

Batch: release-prepare-mutation-inspect

Context:
- Interactive `effigy release prepare` already had staged review steps.
- Mutation review still forced operators to scan the pre-rendered full summary
  block without a focused way to inspect one planned file mutation in detail.

Changes:
- Added a dedicated Step 2 mutation-review loop for interactive prepare.
- Operators can now enter `inspect <n>` or a bare mutation number during
  mutation review to open a focused detail view for that planned file change.
- The detail view shows mutation metadata plus any available concise diff
  preview, then returns cleanly to the mutation-review prompt.
- Updated help and release docs so the new inspection affordance is part of the
  documented interactive workflow.

Verification:
- `cargo test --lib parse_prepare_mutation_inspection_request_accepts_keyword_and_bare_index -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_interactive_ -- --nocapture`

Outcome:
- Movement: baseline `interactive prepare had only summary-level mutation review`
  -> current `interactive prepare supports focused per-mutation drill-down
  before final acceptance`
