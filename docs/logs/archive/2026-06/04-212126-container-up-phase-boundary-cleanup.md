# Container Up Phase Boundary Cleanup

Date: 2026-06-04
Roadmap: `g08.009`
Card: `1040`

## Summary

Completed the container `up` phase-boundary cleanup.

`run_container_up` now reads as orchestration over named phase helpers:
validation and runtime prep, compose execution, backend override persistence,
readiness, runtime integrations, lease cleanup, and final rendering.

## Changes

- Added `ContainerUpPlan` to carry the stable inputs prepared before compose
  execution.
- Added `ContainerUpRuntimeIntegrations` to group gateway, TCP alias, and
  warning output produced after readiness.
- Extracted conflict validation, prepare, compose run, interrupt rendering,
  runtime integration, detached report, and final result helpers.
- Preserved attached interrupt behavior as an early closeout return.
- Left backend selection, gateway registration, TCP alias reconciliation,
  secret materialization, host-process startup, attached sessions, and report
  builders in their existing ownership boundaries.

## Behavior Preservation

- Compose failure still routes through failed-up cleanup.
- Attached interrupt still renders interrupted closeout and stops before
  post-start hooks.
- Readiness and integration failures still clean up through the same failure
  closeout path.
- Detached output still uses the existing container report builder and
  annotation helpers.

## Validation

- `cargo fmt --all`
- `cargo test container_command::lifecycle::tests -- --nocapture`
- `cargo test container_command::closeout::tests -- --nocapture`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `effigy test --plan`

## Vision Target Delta

- Tags: `MAINT`, `OPERATE`
- Baseline: container bring-up mixed validation, runtime prep, compose
  execution, cleanup, integrations, and rendering in one long function.
- Current: the same behavior is split into named phase helpers with shared
  prepared state and grouped runtime integration output.
- Remaining open: repo-marker/root-rule convergence in `g08.009`.

## Next Task

Run ready card `1041`.
