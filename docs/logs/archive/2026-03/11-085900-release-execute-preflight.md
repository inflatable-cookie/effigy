# Release Execute Preflight

Status: complete
Created: 2026-03-11
Roadmap: g01.027
Batch: release-execute-preflight

## Summary

- Added the first `effigy release execute` slice as a non-destructive preflight.
- Execute preflight now consumes `.release-prepared.json` instead of assuming
  preparation state implicitly.
- Kept the safety boundary intact: no commit, tag, push, or state cleanup is
  performed here.

## Changes

- Added `effigy release execute --plan` to the CLI, help output, and release
  command dispatch.
- Implemented prepared-state loading and validation for
  `.release-prepared.json`, including schema/version parsing and required field
  checks.
- Added stale-state warnings using the default one-hour threshold from the
  roadmap’s execute design.
- Verified the git working tree against the prepared file set plus the state
  file, reporting missing expected changes and unexpected extra changes.
- Added JSON/text execute-preflight result payloads so callers can inspect
  readiness, warnings, blockers, and working-tree details.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `Effigy could prepare a release state but had no execute-stage validation` -> current `Effigy can now validate prepared release state and working-tree safety before any irreversible release action`
- Remaining gap: `final approval, commit/tag/push orchestration, cleanup, and full simulate flow remain open`

## Validation Performed

- command: `cargo test --lib parse_release_execute_plan_with_repo -- --nocapture`
  - result: pass
- command: `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_execute_plan_ -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_prepare_ -- --nocapture`
  - result: pass

## Risks

- Execute preflight currently treats the roadmap’s one-hour stale threshold as a
  fixed warning window; making it configurable can be added later if needed.
- The git working-tree verification uses `git status --porcelain=v1`, which is
  sufficient for current prepared-file checks but may need richer rename/path
  handling if execute later stages grow more complex.
- Irreversible release actions remain intentionally absent, so the prepared
  state can only be validated, not executed, through Effigy today.

## Next Task

- Implement the next `g01.027` batch by turning execute preflight into a real
  human-gated execution flow: final approval, non-interactive safety flag,
  commit/tag creation, failure handling around push, and `.release-prepared.json`
  cleanup once execution succeeds.
