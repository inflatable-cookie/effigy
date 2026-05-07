# 501 - Remove Final Runner Compose Runtime Helper Drift

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Remove the final runner-owned compose/runtime helper calls before closing
`g04.004`.

## Scope

- remove direct `compose_args(...)` use from captured exec construction
- move runtime volume/reset helper invocation through manager-owned runtime
  invocation plans
- preserve current CLI behavior and error rendering

## Non-Goals

- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when runner container-command code no longer calls
`compose_args(...)` or `runtime_process_invocation(...)` directly.

## Closeout

Runner container-command code no longer calls `compose_args(...)` or
`runtime_process_invocation(...)` directly.

Captured exec now builds tail args and asks the runtime manager adapter to
construct the compose invocation. Runtime volume/reset helpers now use
manager-owned runtime invocation plans while preserving the previous Colima
default behavior when no backend override is set.

## Validation

- `cargo test -p effigy-container-manager`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Close `g04.004`.
