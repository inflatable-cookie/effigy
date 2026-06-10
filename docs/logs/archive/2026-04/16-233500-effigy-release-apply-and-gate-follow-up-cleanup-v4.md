# 216 Effigy Release Apply And Gate Follow Up Cleanup V4

Created: 2026-04-16
Roadmap: `g02.010`
Batch: `effigy-release-apply-and-gate-follow-up-cleanup-v4`

## Summary

Moved the remaining release apply/gate execution layer out of
`src/runner/release_command.rs` and into `effigy-release`.

This batch turned release apply execution, gate execution with progress, and
standalone gate-run shaping into crate-owned APIs and left the runner much
closer to an honest interactive shell.

## Changes

- widened `crates/effigy-release/src/lib.rs` so `effigy-release` now owns:
  - `run_release_gates_with_progress(...)`
  - `collect_release_gate_run(...)`
  - `execute_release_prepare(...)`
  - `execute_release(...)`
- rewired `src/runner/release_command.rs` onto those crate-owned APIs
- removed the runner-owned duplicated release apply/gate execution helpers
- reduced `src/runner/release_command.rs` from `2181` lines to `1812`

## Vision Target Delta

- Primary tags: `MAINT`, `CONTRACT`, `RELEASE`
- Movement: baseline `release apply/gate execution still inline in runner` -> current `release apply/gate execution lives in effigy-release, leaving a much narrower release runner shell`
- Remaining gap: `src/runner/release_command.rs` still carries interactive
  prepare/execute/resume review flow, prompt and section-browser IO, and final
  runner-side dispatch wiring

## Validation Performed

- command: `cargo test release_command --lib`
  - result: passed
- command: `cargo test --test cli_output_tests release`
  - result: passed
- command: `cargo fmt --all`
  - result: passed
- command: `cargo run --bin effigy -- qa:docs`
  - result: passed
- command: `git diff --check`
  - result: passed

## Risks

- the release shell is materially smaller, but it still needs a strict boundary
  decision before the seam can honestly pause
- the only remaining warning residue in validation is outside the release seam,
  in the parallel demo work

## Next Task

- Execute `217-decide-post-release-apply-and-gate-follow-up-cleanup-v4-boundary.md`.
