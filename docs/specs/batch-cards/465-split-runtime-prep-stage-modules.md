# 465 - Split Runtime Prep Stage Modules

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Split `container_runtime_prep` into stage-owned modules so the new activation
stage seams do not remain concentrated in one file.

## Scope

- keep public runner-facing functions stable:
  - `activate_container_runtime_for_task`
  - `ensure_container_runtime_prepared`
  - `prepare_container_exec_runtime`
- move focused stage code into internal modules, likely:
  - `activation.rs`
  - `validation.rs`
  - `running.rs`
  - `prep.rs`
  - `gateway.rs`
  - `lease.rs`
- keep tests passing with minimal test import churn
- preserve all behavior from cards `455` through `463`

## Non-Goals

- no new behavior
- no caller migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `container_runtime_prep` is mostly orchestration and
exports, with stage ownership split into named modules.

## Closeout

Split runtime activation prep into focused internal modules:

- `validation.rs`
- `running.rs`
- `prep.rs`
- `gateway.rs`
- `lease.rs`

Kept the parent module as the runner-facing shell for:

- `activate_container_runtime_for_task`
- `ensure_container_runtime_prepared`
- `prepare_container_exec_runtime`

No public CLI behavior changed.

## Validation

- `cargo test -p effigy --lib container_runtime_prep`
- `cargo test -p effigy --lib execute`
- `cargo test -p effigy-runtime-plan`
- `git diff --check`

## Next Task

Wire runtime activation planning into workspace sessions.
