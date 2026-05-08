# 486 - Select Exec Shell Or Data Cache Manager Migration

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Choose the next bounded manager-backed migration slice after read and
lifecycle shutdown/reset callers.

## Scope

- inventory remaining direct compose/backend calls in container operation paths
- choose one next slice:
  - exec/shell command execution
  - container data/cache transfer paths
  - attached `container up` session handling
- update lane and roadmap front doors
- do not implement code in this decision card

## Non-Goals

- no public CLI behavior changes
- no code migration
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when one next implementation card is ready.

## Closeout

The next slice is exec/shell manager migration.

Remaining direct-call drift is concentrated in:

- `src/runner/container_command/lifecycle.rs` captured container exec
- `crates/effigy-runtime/src/shell.rs` shell/user-probe/session args
- `crates/effigy-runtime/src/session.rs` attached managed sessions
- `crates/effigy-runtime/src/data.rs` transfer/copy paths
- `src/runner/container_command/support.rs` gateway alias host updates

Exec/shell comes first because it is the highest-churn container-sensitive
surface and directly affects Rhai/container execution behavior.

## Validation

- `git diff --check`

## Next Task

Wire manager compose plans into captured exec callers.
