# 2026-04-16 21:54:47 BST — Effigy Container Runner Shell Follow Up Cleanup

## Summary

`effigy-containers` now owns the container execution/session shaping layer that
was still inline in `src/runner/container_command.rs`.

## What Changed

Added:
- [crates/effigy-containers/src/exec.rs](../../../../crates/effigy-containers/src/exec.rs)
- [crates/effigy-containers/src/session.rs](../../../../crates/effigy-containers/src/session.rs)

Moved into `effigy-containers`:
- Colima running/start checks
- compose `ps` capture
- compose shutdown execution
- generic container command capture
- attached session mode resolution
- attached session tab/process planning
- stream overview rendering
- attached session closeout rendering
- Effigy invocation prefix shaping for session plans

Rewired in:
- [src/runner/container_command/mod.rs](../../../../src/runner/container_command/mod.rs)

## Result

- `src/runner/container_command.rs` dropped from `1064` lines to `790`
- the runner now keeps:
  - CLI command entry and payload shaping
  - inherited child-process spawning
  - signal/process-group shutdown handling
  - final runner error mapping over crate-owned container execution/session APIs

## Churn Check

This was still a real container seam, not helper churn. The remaining runner
weight is now much closer to shell/process handling than container-domain API
ownership.

## Vision Target Delta

- primary vision tags: `MAINT`, `OPERATE`
- moved: container execution and session shaping out of the runner shell and
  into `effigy-containers`
- remaining open: judge whether the surviving runner shell is now an honest
  adapter/process boundary

## Validation

- `cargo test -p effigy-containers`
- `cargo test --test cli_output_tests container`

## Next Task

Execute
[`208-decide-post-container-runner-shell-follow-up-cleanup-boundary.md`](../../../specs/batch-cards/208-decide-post-container-runner-shell-follow-up-cleanup-boundary.md)
to decide whether the remaining container runner shell is honest enough to
pause or whether one more bounded container follow-up is justified.
