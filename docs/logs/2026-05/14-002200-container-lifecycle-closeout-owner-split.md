# Container Lifecycle Closeout Owner Split

Date: 2026-05-14

## Summary

Completed card `729`, the second lifecycle owner split in the reopened cleanup
suite.

## Changes

- added `src/runner/container_command/closeout.rs`
- moved interrupted-up closeout rendering and cleanup failure shaping into the
  new closeout owner
- moved lifecycle reset confirmation into the closeout owner
- kept lifecycle command dispatch stable while shrinking the remaining mixed
  logic in `lifecycle.rs`
- advanced current ready work to card `730`

## Vision Target Delta

- Primary tags: `MAINT`, `OPERATE`
- Baseline: `container_command/lifecycle.rs` still owned cleanup and closeout
  behavior after the earlier secrets and shell-prep split.
- Current state: cleanup and closeout behavior now lives in its own lifecycle
  owner, leaving `lifecycle.rs` more focused on active lifecycle command flow.
- Remaining open: Rhai internal boundary work, CLI help convergence, fixture
  dedup, docs reference refresh, and final closeout.

## Validation

- `cargo test -p effigy interrupted_up_closeout`
- `cargo test -p effigy finish_container_up_failure`
- `cargo fmt --all -- --check`
- `git diff --check`

## Next Task

Execute `730` to extract Rhai internal secrets and process support modules.
