# CLI Output Release Fixture Builder Cleanup

Date: 2026-05-14

## Summary

Completed card `733`, the first area-local test builder cleanup slice.

## Changes

- added private release-fixture helpers in `tests/cli_output_tests/support.rs`
- switched repeated CLI output release fixture writers over to the shared local
  helpers
- kept the cleanup local to one test area instead of introducing a global test
  harness
- advanced current ready work to card `734`

## Vision Target Delta

- Primary tags: `MAINT`
- Baseline: CLI output release tests repeated local changelog and manifest
  fixture setup in multiple nearby release fixture writers.
- Current state: that local release fixture setup now uses shared private
  helpers inside the CLI output test support surface.
- Remaining open: duplicate-proof and residual deferrals, docs reference
  refresh, and final closeout.

## Validation

- `cargo test -p effigy cli_release_prepare_plan_json_mode_includes_sync_file_mutation_when_configured`
- `cargo test -p effigy cli_release_prepare_yes_json_mode_supports_plain_version_file_and_shell_gate`
- `effigy scan duplicate-blocks --json`
- `cargo fmt --all -- --check`
- `git diff --check`

## Validation Notes

- The global duplicate scan still reports the same high findings after this
  slice. That is expected because those remaining highs are now mainly bootstrap
  cross-file setup, release test cross-file ownership, and literal-heavy help
  topic duplication outside this local fixture area.

## Next Task

Execute `734` to capture the duplicate proof and residual deferrals explicitly.
