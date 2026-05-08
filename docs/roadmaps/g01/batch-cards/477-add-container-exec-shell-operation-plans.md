# 477 - Add Container Exec Shell Operation Plans

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Extend `effigy-container-ops` with operation plans for container exec and shell
handoff.

## Scope

- add exec/shell operation variants to `ContainerOperationKind`
- support:
  - captured exec
  - interactive shell
  - command shell
- model side-effect class as runtime interaction, not lifecycle mutation
- preserve no-confirmation policy
- add pure planning tests for command/service/stdin/interactive identity

## Non-Goals

- no runner migration yet
- no backend-manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `effigy-container-ops` exposes exec/shell operation
plans and focused tests pass.

## Closeout

Added exec/shell operation planning for:

- captured exec
- interactive shell
- command shell

These operations now carry an `InteractsWithRuntime` side-effect class and
no-confirm safety policy in the shared operation model.

## Validation

- `cargo test -p effigy-container-ops`
- `git diff --check`

## Next Task

Wire exec/shell operation plans into runner/runtime glue.
