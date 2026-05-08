# 466 - Wire Runtime Activation Plan Into Workspace Sessions

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make workspace container sessions consume the shared runtime activation request
and plan shape instead of calling runtime preparation as a standalone helper.

## Scope

- migrate `src/runner/system_command/workspace_session.rs` to use
  `ActivationRequest`
- preserve workspace session ownership and cleanup behavior
- preserve seeded workspace handoff behavior
- keep gateway-route readiness checks stable so cleanup ownership does not
  drift
- add or update focused workspace session tests if the integration surface
  changes

## Non-Goals

- no public CLI behavior changes
- no workspace UX redesign
- no bootstrap/Rhai migration in this card
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when workspace sessions are activation-plan-backed and
the existing handoff cleanup matrix remains stable.

## Closeout

Workspace sessions now call `activate_container_runtime_for_task` with an
`ActivationRequest`.

Kept cleanup ownership stable by checking gateway-route readiness before
activation, then finishing handoff provisioning/rendering after activation.
Workspace activation deliberately skips host-container lease refresh to preserve
the previous workspace-session behavior.

## Validation

- `cargo test -p effigy --lib workspace`
- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Select the next runtime activation caller migration.
