# 384 - Migrate Container Lifecycle Through Manager

Lane: [`038-plugin-ready-container-manager-facade-strict-lane.md`](../038-plugin-ready-container-manager-facade-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Route container lifecycle commands through `ContainerManager` while preserving
current CLI behavior.

## Scope

- migrate `container up`, `down`, `status`, `stats`, and `logs` command
  construction through manager operations
- keep existing process execution helpers where they still own IO details
- preserve current attached `container up` behavior
- start moving interrupt closeout reporting into manager-owned reports
- add focused tests for lifecycle operation reports and invocation parity
- do not migrate exec/copy/data operations in this card

## Exit Condition

This card is complete when lifecycle command paths consume manager operations,
operation reports include the required identity fields, and remaining direct
backend branching is isolated to exec/copy/data follow-up cards.

## Closeout

Lifecycle paths now create manager-backed operation reports for:

- `container up`
- `container down`
- `container status`
- `container stats --all`
- `container logs`

The report hook lives in `effigy-runtime::container_manager` and uses
`ContainerManager` plus `ContainerBackendDetection` to bind backend id, repo
root, action, state, policy name, cleanup result, and container note.

Existing CLI output remains unchanged.

## Validation

- `CARGO_TARGET_DIR=/tmp/effigy-container-manager-target cargo test -p effigy-container-manager -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-containers-target cargo test -p effigy-containers compose -- --nocapture`
- `CARGO_TARGET_DIR=/tmp/effigy-main-check-target cargo check -p effigy`
- `CARGO_TARGET_DIR=/tmp/effigy-runner-target cargo test -p effigy container_command -- --nocapture`

## Next Task

Implement card `385`: migrate exec, copy, and data container operations
through `ContainerManager`.
