# 560 - Extract Runtime Shell Exec Argument Helpers

Lane: [`050-manager-backed-runtime-read-write-shell-strict-lane.md`](../050-manager-backed-runtime-read-write-shell-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Move shell/exec compose argument construction out of
`crates/effigy-runtime/src/shell.rs`.

## Scope

- create a focused shell module for exec argument construction
- move:
  - non-interactive exec argument rendering
  - interactive shell argument rendering
  - color and workspace user/home env append helpers
- keep public runtime shell functions stable through `shell.rs`
- preserve shell/exec command behavior

## Non-Goals

- no shell behavior changes
- no manager invocation changes
- no task execution routing changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when shell argument helpers are out of `shell.rs`, shell
callers still compile, and focused runtime checks pass.

## Validation

- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-shell-exec-args-check cargo check -p effigy-runtime`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-shell-exec-args-libcheck cargo check -p effigy --lib`
- PASS:
  `CARGO_TARGET_DIR=/tmp/effigy-g04-runtime-shell-exec-args-test-a cargo test -p effigy-runtime shell::exec_args -- --test-threads=1`
- PASS: `git diff --check`

## Next Task

Close manager-backed runtime read/write/shell.
