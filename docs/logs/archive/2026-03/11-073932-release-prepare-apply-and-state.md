# Release Prepare Apply And State

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-prepare-apply-and-state

## Summary

- Extended `effigy release prepare` beyond preview-only mode with an explicit
  non-interactive apply path.
- Supported file mutations are now written to disk and persisted into
  `.release-prepared.json`.
- Kept the safety boundary explicit: no commit, tag, or push is performed here.

## Changes

- Added `--yes` handling for `effigy release prepare` so users can apply the
  prepared version/changelog mutations non-interactively.
- Added prepared-state persistence at `.release-prepared.json` with version,
  tag, release date, timestamp, gate metadata, and modified file list.
- Required `--check-gates` for the apply path whenever `[release.gates]` is
  configured, so the persisted prepared state always reflects a gate-checked
  release.
- Kept `--plan` as the non-destructive preview path and rejected `--plan` +
  `--yes` together.
- Added JSON/text result rendering for successful and failed apply attempts so
  callers can tell whether files were written and whether the state file exists.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `Effigy could preview release mutations but not persist a prepared release state` -> current `Effigy can now write supported release mutations and persist an execute-ready state file without crossing into commit/tag/push operations`
- Remaining gap: `interactive approvals, rerun recovery, sync-file regeneration, and execute-stage git orchestration remain open`

## Validation Performed

- command: `cargo fmt --all`
  - result: pass
- command: `cargo test --lib parse_release_prepare_yes_with_repo_and_gate_check -- --nocapture`
  - result: pass
- command: `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
  - result: pass
- command: `cargo test --lib render_updated_version_contents_supports_json_and_plain_text -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_prepare_ -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_ -- --nocapture`
  - result: pass

## Risks

- TOML and JSON rewrites still go through canonical serializers and do not yet
  preserve original formatting/comments for every file layout.
- If gate execution fails after files are written, the current implementation
  leaves those file changes in place and skips state-file creation; later batches
  can decide whether rollback behavior is worth the added complexity.
- Existing `.release-prepared.json` files currently block another apply attempt
  instead of offering a reconcile/resume flow.

## Next Task

- Implement the next `g01.027` batch by validating and consuming
  `.release-prepared.json` for an `effigy release execute --plan` or equivalent
  preflight, including stale-state detection and working-tree expectation checks
  before any git commit/tag/push logic is introduced.
