# Script Command Owner Sprawl

Date: 2026-05-20  
Roadmap: [`g07.067`](../../../roadmaps/g07/067-script-command-boundary-reduction.md)  
Batch card: [`1017`](../../../roadmaps/g07/batch-cards/1017-reduce-script-command-owner-sprawl.md)  
Strict lane: [`095`](../../../specs/095-residual-maintainability-follow-through-strict-lane.md)

## What Changed

- moved the Rhai feature dispatch and option-decoding surface out of
  [`src/runner/script_command/mod.rs`](../../../../src/runner/script_command/mod.rs)
  into
  [`src/runner/script_command/feature_dispatch.rs`](../../../../src/runner/script_command/feature_dispatch.rs)
- kept runner-local shell/process glue, embedded command parsing, host callback
  wiring, and container activation in `mod.rs`
- updated
  [`src/runner/script_command/tests.rs`](../../../../src/runner/script_command/tests.rs)
  to import the moved dispatch surface from the new module

## Proof

- `script_command/mod.rs`: `1684` lines -> `319`
- `effigy scan god-files --json`: `0` findings
- `cargo fmt --all -- --check`: pass
- focused runner proof:
  - `CARGO_TARGET_DIR=/tmp/effigy-1017-target cargo test -p effigy --lib every_registered_rhai_feature_has_a_runner_dispatch_branch -- --nocapture`
  - `CARGO_TARGET_DIR=/tmp/effigy-1017-target cargo test -p effigy --lib run_rhai_feature_dispatches_deploy_model_for_fixture_repo -- --nocapture`
  - `CARGO_TARGET_DIR=/tmp/effigy-1017-target cargo test -p effigy --lib run_rhai_feature_preserves_deploy_apply_confirmation_guard -- --nocapture`
  - `CARGO_TARGET_DIR=/tmp/effigy-1017-target cargo test -p effigy --lib parse_rhai_embedded_command_defaults_repo_override_when_missing -- --nocapture`

## Notes

The earlier "stalled" top-level `effigy` build was local target-dir drag, not a
semantic failure in this change. Sampling showed `rustc` spending time walking
the very large existing `target/debug/deps` directory while building search
paths. Re-running the focused proof in a fresh `CARGO_TARGET_DIR` compiled and
ran cleanly.

## Vision Target Delta

- primary vision tags touched: `MAINT`, `OPERATE`
- moved in this report: runner god-file debt `1 -> 0`; `script_command/mod.rs`
  now owns only runner-local glue instead of the full Rhai dispatch surface
- remains open:
  - `1018`: help-topic duplicate reduction
  - `1019`: language-emitter duplicate follow-through
  - `1020`: runner fixture/helper convergence
  - `1021`: residual maintainability closeout
