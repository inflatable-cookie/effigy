# Rhai Host-Surface Test Ownership

Date: 2026-05-14
Roadmap: `g05.020`
Batch card: `747`

## What changed

- replaced the single `crates/effigy-rhai/src/tests.rs` god file with:
  - `crates/effigy-rhai/src/tests/mod.rs`
  - `crates/effigy-rhai/src/tests/secrets.rs`
  - `crates/effigy-rhai/src/tests/host_surface.rs`
  - `crates/effigy-rhai/src/tests/runtime.rs`
  - `crates/effigy-rhai/src/tests/utility.rs`
  - `crates/effigy-rhai/src/tests/script_policy.rs`
- kept shared fixtures and repo-wide Rhai script-policy helpers in `tests/mod.rs`
- preserved deploy-provider, state-context, runtime-context, and structured-data
  coverage after the split
- updated the first-party process allowlist for
  `external/bundles/underlay/scripts/dev/ui-setup.rhai`

## Validation

- `cargo test -p effigy-rhai`
- `cargo fmt --all -- --check`
- `effigy scan god-files --json`
- `git diff --check`

## Outcome

The Rhai test surface is now grouped by owner seam instead of accumulating into
one file. The god-file scan now reports only:

- `src/runner/state_command.rs`
- `crates/effigy-release/src/lib.rs`
