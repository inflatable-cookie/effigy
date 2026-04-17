# 226 Implement Effigy Bootstrap Integration Test Ownership

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the crate-domain bootstrap integration tests out of the runner and into
`crates/effigy-bootstrap/tests/` so the remaining runner test module only
exercises the actual runner path (`run_bootstrap_with_cwd`).

## In Scope

- relocate git-fixture bootstrap integration tests from
  `src/runner/bootstrap_command.rs` to
  `crates/effigy-bootstrap/tests/integration.rs`
- adapt the tests to drive `execute_bootstrap_request` directly with `sh`-based
  `run_task` and `load_task_manifest`-based `load_bootstrap` callbacks
- drop runner-local duplicate fixture helpers that only the moved tests used
- leave only full-shell `run_bootstrap_with_cwd_*` tests in the runner module

## Out Of Scope

- release execution
- demo/docs/container parallel cleanup
- new crate work outside `effigy-bootstrap`

## Acceptance Criteria

- `src/runner/bootstrap_command.rs` loses the crate-domain git-fixture tests
  and the fixture helpers that only they used
- `crates/effigy-bootstrap/tests/integration.rs` exercises
  `execute_bootstrap_request` against real git remotes without touching the
  runner
- runner test module keeps only `run_bootstrap_with_cwd_*` full-shell tests
- `cargo test` green across the workspace

## Validation

- `cargo test`
- `cargo fmt --all -- --check`

## Next Task

Execute
[`227-decide-post-bootstrap-integration-test-ownership-boundary.md`](./227-decide-post-bootstrap-integration-test-ownership-boundary.md)
to decide whether bootstrap can now pause on an honest shell boundary.
