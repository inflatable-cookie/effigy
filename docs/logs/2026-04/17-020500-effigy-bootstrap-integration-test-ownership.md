# 2026-04-17 02:05:00 BST — Effigy Bootstrap Integration Test Ownership

## Summary

Moved the crate-domain bootstrap integration tests out of the runner and into
`crates/effigy-bootstrap/tests/integration.rs`. The runner's
`bootstrap_command.rs` test module now only exercises the full runner path
(`run_bootstrap_with_cwd`) end-to-end; everything that was actually testing
`execute_bootstrap_request` against real git remotes now lives with the crate.

`src/runner/bootstrap_command.rs` dropped from `628` → `402` lines.

## Why This Batch

Post-`224` boundary inspection showed the runner module was still carrying
~540 lines of git-fixture helpers and `execute_bootstrap_request` assertions
that had nothing to do with the runner's own wiring. Those tests were only
reachable through thin pass-through wrappers the runner kept for that purpose.
Moving them to the crate is a small, honest ownership fix.

## What Changed

- added `crates/effigy-bootstrap/tests/integration.rs` with:
  - `temp_dir`, `init_git_repo`, `commit_all`, bare/clone remote helpers
  - fixture builders for root/child/optional-child/plain-root remotes
  - a `load_bootstrap_from_manifest` callback that parses the manifest via
    `effigy-manifest`
  - a `run_task_via_sh` callback that resolves the task's `run` command and
    executes it through `sh -c`
  - five tests covering clone + setup + children, remote mismatch refusal,
    dirty-checkout refusal, optional child warnings, and missing-contract
    rendering
- trimmed `src/runner/bootstrap_command.rs` test module to only keep
  `run_bootstrap_with_cwd_starts_when_requested` and
  `run_bootstrap_with_cwd_reports_optional_child_warning_in_text_output`
- removed the git-fixture helpers that only the moved tests used

## Churn Check

This is not shuffling code for shape. The crate now owns crate-domain testing
for its own public surface, the runner keeps only runner-path integration
tests, and the runner module got materially smaller without losing coverage.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: crate-domain bootstrap test ownership is now aligned with the crate
- remaining open: post-`226` boundary decision for the remaining bootstrap
  shell

## Validation

- `cargo test -p effigy-bootstrap` — all bootstrap tests (5 unit + 5
  integration) green
- `cargo test` — full workspace green (11 test suites, 0 failures)
- `cargo fmt --all -- --check`

## Next Task

Execute
[`227-decide-post-bootstrap-integration-test-ownership-boundary.md`](../../specs/batch-cards/227-decide-post-bootstrap-integration-test-ownership-boundary.md)
to decide whether bootstrap can now pause on an honest shell boundary.
