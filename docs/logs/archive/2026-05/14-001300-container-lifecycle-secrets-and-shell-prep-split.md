# Container Lifecycle Secrets And Shell Prep Split

Date: 2026-05-14

## Summary

Completed card `728`, the first real lifecycle owner split in the reopened
cleanup suite.

## Changes

- added `src/runner/container_command/secret_env.rs`
- added `src/runner/container_command/shell_prep.rs`
- moved container secret env resolution into the new secret owner
- moved shell session prep, workspace refresh checks, exec env assembly, and
  working-dir mapping into the new shell-prep owner
- reduced `lifecycle.rs` to less mixed ownership around secrets and shell prep
- advanced current ready work to card `729`

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`
- Baseline: `container_command/lifecycle.rs` still mixed lifecycle dispatch with
  container secret env resolution and shell/exec prep ownership.
- Current state: those two owners now live in dedicated modules with focused
  tests, while lifecycle dispatch and remaining cleanup flow stay in
  `lifecycle.rs`.
- Remaining open: lifecycle cleanup/closeout helpers, Rhai internal boundary
  work, CLI help convergence, fixture dedup, docs reference refresh, and final
  closeout.

## Validation

- `cargo test -p effigy container_secret_env`
- `cargo test -p effigy explicit_exec_working_dir`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `729` to split remaining lifecycle cleanup and closeout helpers.
