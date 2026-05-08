# 483 - Add Manager Compose Invocation Plan Foundation

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Give container operation migrations one manager-owned compose invocation plan
instead of continuing to call lower-level compose/backend helpers directly.

## Scope

- add a dependency-light compose invocation plan type to
  `effigy-container-manager`
- include backend id, repo root, profile, action, program, args, and label
- preserve current Docker and Colima invocation shapes
- add focused manager tests for Docker and Colima plans
- do not migrate runner/runtime callers yet

## Non-Goals

- no public CLI behavior changes
- no side-effect execution in the new plan type
- no data/seed pipeline extraction
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when manager-owned compose invocation plans exist and are
covered by focused tests.

## Closeout

`effigy-container-manager` now owns a `ContainerComposeInvocationPlan` that
captures backend id, repo root, profile, action, program, args, and label.

The plan preserves current Docker and Colima invocation shapes and honors an
explicit manager request backend override before environment/path detection.

## Validation

- `cargo test -p effigy-container-manager`
- `git diff --check`

## Next Task

Wire the compose invocation plan into read-only runtime callers.
