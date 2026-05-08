# 544 - Extract Container Exec Process Module

Lane: [`049-effective-container-policy-decomposition-strict-lane.md`](../049-effective-container-policy-decomposition-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Continue splitting container exec by moving process spawning, capture,
streaming, timeout, and termination helpers into a focused module.

## Scope

- create `crates/effigy-containers/src/exec/process.rs`
- move process helper functions where dependencies stay clean:
  - command capture helpers
  - timeout capture/stream helpers
  - spawn helpers
  - process-tree termination helpers
  - timeout error detection
- keep public `run_command_capture` and `run_command_capture_allow_failure`
  exports stable through `exec.rs`
- preserve timeout behavior and tests

## Non-Goals

- no Colima recovery behavior changes
- no Docker/Colima backend migration
- no parser changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when process-side helpers are out of
`exec/implementation.rs`, public exec APIs still compile, and focused timeout
tests pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-process-check cargo check -p effigy-containers`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-process-libcheck cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-process-test-a cargo test -p effigy-containers command_timeout_message_mentions_elapsed_seconds -- --test-threads=1`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-process-test-b cargo test -p effigy-containers streamed_command_failure_reports_streaming_footer -- --test-threads=1`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-exec-process-test-c cargo test -p effigy-containers timeout_detection_matches_timeout_footer -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Extract Colima runtime and repair helpers out of the remaining exec
implementation module.
