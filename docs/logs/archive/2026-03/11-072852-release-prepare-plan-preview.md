# Release Prepare Plan Preview

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-prepare-plan-preview

## Summary

- Added the first non-destructive `effigy release prepare --plan` flow.
- Reused the release-status foundation to preview version-file and changelog
  mutations without touching the working tree.
- Extended roadmap `027` and release protocol docs to reflect the new prepare
  planning surface while leaving interactive/apply behavior explicitly open.

## Changes

- Extended the release parser and command model with `release prepare --plan`
  plus optional `--check-gates` support.
- Refactored `src/runner/release_command.rs` around a shared release context so
  `status` and `prepare --plan` use the same config loading, version reading,
  changelog validation, bump analysis, and gate execution logic.
- Added internal mutation renderers for supported version file formats and
  changelog preparation previews:
  - version-file content rendering for `Cargo.toml`, `package.json`,
    `pyproject.toml`, and `VERSION`
  - changelog rendering that promotes `[Unreleased]` into a dated release while
    resetting `[Unreleased]`
- Added JSON/text plan output describing planned file mutations and blocker
  state.
- Updated help text, release protocol docs, roadmap progress, and changelog
  notes to include `effigy release prepare --plan`.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `release readiness could be inspected, but file mutations still lived only in shell scripts` -> current `Effigy can now preview the exact version/changelog mutation plan before any release preparation writes occur`
- Remaining gap: `interactive approvals, actual file writes during prepare, prepared-state persistence, and execute/tag/push orchestration remain open`

## Validation Performed

- command: `cargo fmt --all`
  - result: pass
- command: `cargo test --lib parse_release_prepare_plan_with_repo_and_gate_check -- --nocapture`
  - result: pass
- command: `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
  - result: pass
- command: `cargo test --lib render_updated_version_contents_supports_json_and_plain_text -- --nocapture`
  - result: pass
- command: `cargo test --lib render_prepared_changelog_moves_unreleased_entries_into_new_release -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_ -- --nocapture`
  - result: pass

## Risks

- Version-file mutation rendering currently uses canonical serializers
  (`toml::to_string_pretty`, `serde_json::to_string_pretty`) and does not yet
  preserve original formatting/comments for an eventual apply path.
- `prepare --plan` intentionally stops short of writing files or creating a
  `.release-prepared.json` state file, so operators must still use the legacy
  scripts for real release preparation.
- Gate execution still shells through the user's default shell rather than the
  task runner's broader process/runtime abstractions.

## Next Task

- Implement the next `g01.027` batch by turning the current renderers into real
  apply/write operations, then add prepared-state file creation so `effigy
  release prepare` can persist an approved plan without committing, tagging, or
  pushing.
