# 050 - Manager Backed Runtime Read Write Shell Strict Lane

Roadmap: [`g04.008`](../roadmaps/g04/008-manager-backed-runtime-read-write-shell.md)

Status: Active
Owner: Platform
Created: 2026-05-07

## Purpose

Remove remaining old compose/process command construction from
`effigy-runtime` by routing read, write, shell, and data helpers through
manager-backed invocation seams.

## Hard Boundaries

- preserve public runtime helper behavior unless a card selects a cleanup break
- prefer manager-backed invocation plans over raw compose helper calls
- no broad formatting churn
- no release work
- no `.github/workflows/` edits

## Current Ready Card

[`554-extract-runtime-data-transfer-validation.md`](./batch-cards/554-extract-runtime-data-transfer-validation.md)

## Execution Chain

- `546` complete: close effective container policy decomposition
- `547` complete: scaffold manager-backed runtime read/write/shell lane
- `548` complete: rename runtime signal compose helpers
- `549` complete: move runtime image cleanup capture behind a signal helper
- `550` complete: remove unused args-based compose signal helpers
- `551` complete: extract runtime data volume/cache planning helpers
- `552` complete: extract runtime data volume IO helpers
- `553` complete: extract runtime data report helpers
- `554` ready: extract runtime data transfer validation helpers

## Drift Inventory

Initial scan targets:

- `crates/effigy-runtime/src/read.rs`
- `crates/effigy-runtime/src/write.rs`
- `crates/effigy-runtime/src/shell.rs`
- `crates/effigy-runtime/src/data.rs`
- `crates/effigy-runtime/src/signals.rs`

Initial forbidden-call checks:

- `compose_args(`: no matches in `crates/effigy-runtime/src/{read,write,shell,data,signals}.rs`
- `run_docker_capture`: removed from `crates/effigy-runtime/src` and
  `src/runner` by card `548`
- compose-plan capture:
  - `read.rs` status/logs paths already use `compose_invocation_plan` plus
    `run_compose_plan_capture`
  - `write.rs` down/reset paths already use compose invocation plans, but still
    route through signal helpers for side effects
  - `data.rs` compose-up for pull-production uses `compose_up_invocation_plan`
    plus `run_compose_plan_capture`
- raw command construction:
  - `signals.rs` owns `Command::new(...)` at the named process boundary for
    plan capture and inherited compose sessions
  - `write.rs` no longer owns inline `Command::new(...)` after card `549`
- Docker-named runtime signal helper exports: removed by card `548`
- unused args-based compose signal helpers: removed by card `550`

First implementation slice:

Rename runtime signal helpers away from Docker-specific names and migrate runner
callers to manager-plan-compatible names while preserving behavior.

## Exit Condition

This lane closes when runtime read/write/shell/data helpers no longer rely on
old compose/process command construction outside named manager adapter seams,
operation reports remain compatible, and focused runtime tests pass.

## Next Task

Card
[`554-extract-runtime-data-transfer-validation.md`](./batch-cards/554-extract-runtime-data-transfer-validation.md).
