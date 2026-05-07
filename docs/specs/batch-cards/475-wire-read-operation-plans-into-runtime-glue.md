# 475 - Wire Read Operation Plans Into Runtime Glue

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make status, logs, and stats runtime paths build typed read operation plans
before side effects run.

## Scope

- wire `status`, `logs`, and `stats` plan construction into
  `crates/effigy-runtime/src/read.rs`
- preserve current CLI behavior and JSON output
- keep existing Docker/Colima execution helpers in place
- add focused tests around read plan identity if the helper is exposed for tests

## Non-Goals

- no backend-manager migration yet
- no exec/shell/data/cache migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when read-only runtime operations construct typed plans
and focused container command tests remain stable.

## Closeout

Runtime read paths now construct typed read operation plans for:

- single-container `status`
- `status --all` and scoped status discovery
- `logs`
- `stats --all`

The existing runtime command execution and report rendering paths are unchanged.

## Validation

- `cargo test -p effigy-container-ops`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Select exec/shell, data/cache, or backend-manager migration as the next
operation slice.
