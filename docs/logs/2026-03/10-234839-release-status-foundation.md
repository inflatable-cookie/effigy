# Release Status Foundation

Status: complete
Created: 2026-03-10
Roadmap: g01.027
Batch: release-status-foundation

## Summary

- Verified roadmap `026` is complete as the shipped changelog-library
  foundation and updated its status/closeout note accordingly.
- Implemented the first real `027` command slice: `effigy release status`.
- Added `[release]` manifest config parsing, version-file autodetection,
  changelog readiness checks, optional gate execution, and JSON payload output.

## Changes

- Added `release` as a first-class command with parser, help, labels, repo
  override handling, JSON-mode wiring, and runner dispatch.
- Added `[release]` config support in the manifest layer, including gate
  shorthand/table forms, kebab-case fields, version-file/path handling, and
  basic config validation.
- Implemented release status collection in `src/runner/release_command.rs`:
  - version-file detection for `Cargo.toml`, `package.json`,
    `pyproject.toml`, and `VERSION`
  - changelog parse/validate/analyze checks using Effigy's changelog library
  - optional gate execution with structured JSON results
  - readiness/blocker reporting with exit-code semantics
- Added targeted parse/help/json unit coverage plus end-to-end CLI envelope
  tests for ready and gate-failure paths.
- Updated the release protocol guide, changelog, and roadmap `027` checklist to
  reflect the new shipped `release status` surface.

## Vision Target Delta

- Primary tags: `RELEASE`, `OPERATE`, `MAINT`
- Movement: baseline `release orchestration existed only as scripts and a roadmap sketch` -> current `Effigy now ships a first-party release readiness command with config, changelog analysis, and gate checks`
- Remaining gap: `prepare`, `execute`, version-file writes, sync-file updates, and migration off legacy release scripts remain open in roadmap 027`

## Validation Performed

- command: `cargo fmt --all`
  - result: pass
- command: `cargo test release_option_tests -- --nocapture`
  - result: pass
- command: `cargo test --lib render_release_help_shows_status_and_gate_options -- --nocapture`
  - result: pass
- command: `cargo test --lib suggested_bump_respects_pre_1_0_breaking_policy -- --nocapture`
  - result: pass
- command: `cargo test --lib apply_global_json_flag_sets_non_task_command_json_mode -- --nocapture`
  - result: pass
- command: `cargo test --test cli_output_tests cli_release_status_json_mode -- --nocapture`
  - result: pass

## Risks

- `release status` currently reads versions across supported file types but does
  not yet write them back; prepare/execute flows still depend on existing
  scripts.
- Gate execution currently shells through the user's default POSIX-style shell
  and does not yet reuse task-runner shell configuration or richer process
  streaming/reporting.
- Roadmap `026` is closed as the library foundation, but older script/workflow
  migration bullets are intentionally treated as release-orchestration follow-up
  work under roadmap `027`.

## Next Task

- Implement the next `g01.027` orchestration batch by adding version-file write
  support plus changelog/version mutation previewing, then use that foundation
  to land the first non-destructive `effigy release prepare --plan` flow without
  tagging or pushing.
