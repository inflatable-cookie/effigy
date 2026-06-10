# Effigy Demo Execution Runtime And Attempt Follow-up Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`169` moves the shared demo execution-attempt and log-path layer into
[crates/effigy-demo/src/execution.rs](../../../../crates/effigy-demo/src/execution.rs).

[src/runner/demo_command/mod.rs](../../../../src/runner/demo_command/mod.rs)
no longer owns `DemoExecutionAttempt`, `DemoLogPaths`, or the receipt/log
persistence shaping for executed attempts directly. The runner now adapts those
crate-owned contracts through thinner wrappers while keeping the raw process
loop and host orchestration local.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `demo runner still owns attempt/log execution contract` ->
  current `demo attempt/log execution ownership moved behind effigy-demo`
- Remaining gap: `demo_command still owns raw process/runtime orchestration and
  launch control`

## Evidence

- new shared crate module:
  [crates/effigy-demo/src/execution.rs](../../../../crates/effigy-demo/src/execution.rs)
- runner file reduction:
  [src/runner/demo_command/mod.rs](../../../../src/runner/demo_command/mod.rs)
  `3964 -> 3804` lines
- moved contracts:
  - `DemoExecutionAttempt`
  - `DemoLogPaths`
  - attempt result constructors
  - receipt persistence shaping
  - output-log persistence shaping

## Validation

- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`
- `cargo test --test cli_output_tests demo`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`170-decide-post-demo-execution-runtime-and-attempt-follow-up-boundary.md`.
