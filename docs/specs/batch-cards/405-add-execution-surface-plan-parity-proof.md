# 405 - Add Execution Surface Plan Parity Proof

Lane: [`040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md`](../040-dependability-proof-matrix-for-decodelabs-and-underlay-shapes-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Prove direct task, bootstrap task, and Rhai task entrypoints produce equivalent
resolved execution plans for the same selector/context inputs.

## Scope

- add or tighten focused `effigy-execution` proof coverage
- compare direct CLI, bootstrap, and Rhai surfaces for the same task selector,
  runtime context, runtime policy, and env/stdin inputs
- assert route and environment parity while preserving each surface label
- no public CLI behavior changes

## Exit Condition

This card is complete when execution-plan parity fails if direct, bootstrap, or
Rhai task request construction diverges for equivalent inputs.

## Closeout

Added an `effigy-execution` parity proof for direct CLI, bootstrap, and Rhai
task surfaces.

The proof builds three plans from the same runtime context, selector, args,
runtime policy, output mode, env, cwd, and `stdin_file`. It asserts route,
invocation, runtime policy, environment, and captured context parity while
preserving each surface label.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-execution direct_bootstrap_and_rhai -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo test -p effigy-execution -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `git diff --check`

## Next Task

Decide whether `g03.034` can close or needs one final proof slice.
