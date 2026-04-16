# Effigy Demo Runtime Control And Process Follow-up Extraction

Date: 2026-04-16
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Summary

`171` moves the shared demo runtime-control/process helper layer into
[crates/effigy-demo/src/process.rs](/Users/tom/Dev/projects/effigy/crates/effigy-demo/src/process.rs).

[src/runner/demo_command.rs](/Users/tom/Dev/projects/effigy/src/runner/demo_command.rs)
no longer owns the run-backed launch mode surface, terminal sizing helpers,
PTY wrapping, output capture helpers, or input handoff forwarding helpers
directly. The runner now adapts those crate-owned process helpers while
keeping the managed runtime event loop and host orchestration local.

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`
- Movement: baseline `demo runner still owns shared runtime-control/process helper layer` ->
  current `demo runtime-control/process helper ownership moved behind effigy-demo`
- Remaining gap: `demo_command still owns managed runtime state/event loop and
  runtime backend classification`

## Evidence

- new shared crate module:
  [crates/effigy-demo/src/process.rs](/Users/tom/Dev/projects/effigy/crates/effigy-demo/src/process.rs)
- runner file reduction:
  [src/runner/demo_command.rs](/Users/tom/Dev/projects/effigy/src/runner/demo_command.rs)
  `3804 -> 3527` lines
- moved contracts:
  - `DemoLaunchMode`
  - launch-mode resolution and terminal sizing helpers
  - PTY wrapping
  - output capture helpers
  - stdin forwarding and handoff helpers
  - transcript sanitizing

## Validation

- `cargo test -p effigy-demo`
- `cargo test demo_command --lib`
- `cargo test --test cli_output_tests demo`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
`172-decide-post-demo-runtime-control-and-process-follow-up-boundary.md`.
