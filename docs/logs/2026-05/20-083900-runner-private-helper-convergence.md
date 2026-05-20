# Runner Private Helper Convergence

Date: 2026-05-20  
Roadmap: [`g07.070`](../../roadmaps/g07/070-runner-private-fixture-and-helper-convergence.md)  
Batch card: [`1020`](../../roadmaps/g07/batch-cards/1020-converge-runner-private-fixtures-and-helpers.md)  
Strict lane: [`095`](../../specs/095-residual-maintainability-follow-through-strict-lane.md)

## What Changed

- added a local runner test helper in
  [`src/runner/container_command/test_support.rs`](../../../src/runner/container_command/test_support.rs)
  for container-command temp repo creation
- rewired the duplicated temp-repo setup in:
  - [`lifecycle.rs`](../../../src/runner/container_command/lifecycle.rs)
  - [`shell_prep.rs`](../../../src/runner/container_command/shell_prep.rs)
- kept the helper private to `container_command` test ownership instead of
  starting a wider cross-runner fixture abstraction

## Proof

- `cargo fmt --all -- --check`: pass
- focused runner proof:
  - `CARGO_TARGET_DIR=/tmp/effigy-1020-target cargo test -p effigy --lib non_primary_service_exec_does_not_force_primary_working_dir -- --nocapture`
  - `CARGO_TARGET_DIR=/tmp/effigy-1020-target cargo test -p effigy --lib run_container_eject_promotes_generated_compose -- --nocapture`
- duplicate scan delta:
  - `effigy scan duplicate-blocks --json`
  - findings: `103 -> 103`
  - high findings: `1 -> 0`
  - no high or critical duplicate blocks remain

## Notes

The duplicate scan now reports only warning-level blocks. The old high pair was
real maintenance debt because it was the same helper living in two sibling
runner test owners. The replacement helper is still close to the owning
surface, so the cleanup did not turn into a generic fixture library.

## Vision Target Delta

- primary vision tags touched: `MAINT`, `OPERATE`
- moved in this report: high duplicate runner-helper findings `1 -> 0`; the
  residual maintainability lane now has no high or critical duplicate-block
  findings left
- remains open:
  - `1021`: residual maintainability closeout
