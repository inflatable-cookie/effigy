# 2026-04-17 01:50:00 BST — Post Bootstrap Runner Shell Follow Up Cleanup V2 Boundary Decision

## Summary

The bootstrap runner shell is close to paused, but not yet. After `224` moved
the plan/result projection into `effigy-bootstrap`, `bootstrap_command.rs`
still carries ~540 lines of crate-domain integration tests (git fixtures +
`execute_bootstrap_request` assertions) that do not actually exercise the
runner path.

One more bounded slice is justified — move those tests into the crate so the
remaining runner module is honest shell + runner-path integration only.

## Why This Decision

Card `224` cleaned the production shell cleanly, but the test surface tells a
different story:

- Crate-level tests assert clone state, manifest discovery, submodule policy,
  optional child failures, dirty-checkout refusal, and plan/result rendering.
- None of these touch `run_bootstrap_with_cwd` or the runner's
  `load_task_manifest` + `run_manifest_task_with_cwd` wiring.
- They exercise `execute_bootstrap_request` directly and only need
  `effigy-manifest` + `sh -c` to drive tasks.

That is crate-domain testing sitting in the wrong place. Moving it is a real,
bounded slice — not churn.

## Decision

- open card `226` to move the crate-domain bootstrap tests into
  `crates/effigy-bootstrap/tests/integration.rs`
- leave only `run_bootstrap_with_cwd_*` runner-path integration tests in the
  runner module
- after `226`, open `227` to re-check the boundary and pause bootstrap cleanly
  if the remaining shell is genuinely adapter work

## Churn Check

This is not invented polish. The runner module is currently 803 lines — 86
lines of shell code and the rest is test fixtures and crate-domain assertions.
Moving the tests to the crate removes ~400 lines from the runner and co-locates
crate-domain coverage with the crate.

## Vision Target Delta

- primary vision tags: `CONTRACT`, `MAINT`
- moved: bootstrap boundary decision is explicit and one more bounded slice
  (test ownership) is scoped
- remaining open: execute `226`, then classify the post-`226` boundary in `227`

## Validation

- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`226-implement-effigy-bootstrap-integration-test-ownership.md`](../../specs/batch-cards/226-implement-effigy-bootstrap-integration-test-ownership.md)
to move the crate-domain bootstrap tests out of the runner shell.
