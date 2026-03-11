## 2026-03-11 13:23:00 - Release preview diff snippets

Batch: release-preview-diff-snippets

Context:
- `effigy release simulate` and `effigy release prepare --plan` already showed
  mutation summaries plus one-line before/after previews.
- The remaining operator-review gap was preview fidelity: there was no concise
  inline diff snippet or mutation-detail payload for per-file review.

Changes:
- Extended release mutation planning to carry `detail_lines` and `diff_preview`
  data for supported previewable mutations.
- Added concise inline diff generation for version-file and changelog write
  mutations, with truncation and change-count limiting to keep previews short.
- Updated text-mode mutation review for simulate, prepare plan, and interactive
  prepare review to render mutation details plus inline diff snippets.
- Extended JSON payloads for `effigy.release.simulate.v1` and
  `effigy.release.prepare.plan.v1` to include `detail_lines` and
  `diff_preview` per mutation.
- Added coverage for the diff helper, JSON mutation payloads, and a text-mode
  simulate contract proving diff snippets are shown to operators.

Verification:
- `cargo test --lib build_diff_preview_limits_to_concise_changed_lines -- --nocapture`
- `cargo test --test cli_output_tests cli_release_simulate_ -- --nocapture`
- `cargo test --test cli_output_tests cli_release_prepare_plan_json_mode_ -- --nocapture`

Outcome:
- Movement: baseline `preview surfaces showed only summary + before/after lines`
  -> current `preview surfaces now expose richer per-file details and concise
  inline diffs without changing release semantics or writing extra state`
