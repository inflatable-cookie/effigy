# State Command Render Owner Split

Date: 2026-05-13

## Summary

Completed card `724`, the second `g05` state thin-shell slice.

## Changes

- added `src/runner/state_command_render.rs` as the runner-owned text rendering
  owner for state command reports
- removed the plan/apply/capture/capture-set/history text renderers from
  `state_command.rs`
- kept the state command entrypoint focused on orchestration and side effects
- advanced current ready work to card `725`

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`
- Baseline: `state_command.rs` still mixed command orchestration with a large
  block of runner-owned text rendering.
- Current state: state text rendering lives in its own runner owner and
  `state_command.rs` shrank again without command behavior drift.
- Remaining open: shared vault-access convergence, container lifecycle
  follow-through, Rhai internal boundary work, CLI help convergence, fixture
  dedup, and final docs/closeout cleanup.

## Validation

- `cargo test -p effigy state_command`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `725` to open the shared secrets vault access lane.
