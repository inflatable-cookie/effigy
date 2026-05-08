# 481 - Wire Data Cache Operation Plans Into Runtime Glue

Lane: [`046-container-operation-pipeline-strict-lane.md`](../046-container-operation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Make container data and cache runtime paths build typed operation plans before
side effects run.

## Scope

- wire plans into `crates/effigy-runtime/src/data.rs`
- wire prompt-facing data seed/dump/pull-production runner paths where they
  live outside runtime data helpers
- preserve current CLI behavior and JSON output
- keep existing volume/archive/OCI execution paths unchanged
- add focused tests around data/cache plan identity where helpers are exposed

## Non-Goals

- no backend-manager migration yet
- no public CLI behavior changes
- no data pipeline extraction
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when data/cache runtime surfaces construct typed plans
and focused container command tests pass.

## Closeout

Data/cache paths now construct typed operation plans for:

- data list/export/import/pull-production/seed/dump
- cache list/prune
- profile-wide cache list/prune

The existing volume, archive, OCI, prompt, and report behavior is unchanged.

## Validation

- `cargo test -p effigy-container-ops`
- `cargo test -p effigy --lib container_command`
- `git diff --check`

## Next Task

Select backend-manager migration or `g04.004` closeout.
