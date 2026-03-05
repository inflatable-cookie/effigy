# 2026-03-01 - Shell Completion and Command Discovery Validation

Date: 2026-03-01
Owner: Platform
Related roadmap: shell completion and command discovery polish

## Scope

Ship shell completion generation with command-discovery parity:
- add `effigy completion <bash|zsh|fish> [--json]`
- ensure completion command list tracks built-in command index (`BUILTIN_TASKS`)
- update help/docs/command matrix and release smoke scripts
- validate completion command behavior and JSON contracts

## Changes

- Added built-in completion command implementation:
  - file: `src/runner/builtin/completion.rs`
  - supports `bash`, `zsh`, `fish`
  - supports `--json` (`effigy.completion.v1`) and `--help`
- Wired completion command into built-in dispatch:
  - file: `src/runner/builtin/mod.rs`
- Added completion to built-in command index:
  - file: `src/runner/model.rs`
- Updated general help command table to include `effigy completion`:
  - file: `src/cli_help/topics/general.rs`
- Added test coverage:
  - `run_manifest_task_builtin_completion_help_renders_topic`
  - `run_manifest_task_builtin_completion_bash_outputs_script`
  - `run_manifest_task_builtin_completion_json_uses_completion_schema`
  - `builtin_completion_json_contract_has_versioned_shape`
- Updated docs and JSON contract index:
  - `README.md`
  - `docs/guides/017-json-output-contracts.md`
  - `docs/guides/021-quick-start-and-command-cookbook.md`
  - `docs/guides/025-command-reference-matrix.md`
  - `docs/guides/026-json-payload-examples.md`
  - `docs/contracts/json-schema-index.json`
- Updated release smoke/install checks to exercise completion:
  - `scripts/check-release-smoke.sh`
  - `scripts/check-release-install-from-tag.sh`

## Validation

- command: `cargo test run_manifest_task_builtin_completion_ -- --test-threads=1`
  - result: pass (3/3)
- command: `cargo test builtin_completion_json_contract_has_versioned_shape -- --test-threads=1`
  - result: pass
- command: `cargo test render_help_writes_structured_sections -- --test-threads=1`
  - result: pass
- command: `./scripts/check-json-contracts.sh --fast`
  - result: pass; includes `effigy --json completion bash` row validation in `effigy.command.v1` envelope checks

## Risks / Follow-ups

- Completion currently focuses on command and option token suggestions; it does not attempt dynamic task-name completion from workspace manifests.
- If built-in command option shapes change, completion option tables should be updated alongside command docs/tests.

## Next

- Add dynamic task selector completion (optional phase 2): complete `<task>` and `<catalog>/<task>` from `effigy tasks --json` data when running in a real workspace.
