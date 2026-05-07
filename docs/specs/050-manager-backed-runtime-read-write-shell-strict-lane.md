# 050 - Manager Backed Runtime Read Write Shell Strict Lane

Roadmap: [`g04.008`](../roadmaps/g04/008-manager-backed-runtime-read-write-shell.md)

Status: Complete
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

None. Lane complete.

## Execution Chain

- `546` complete: close effective container policy decomposition
- `547` complete: scaffold manager-backed runtime read/write/shell lane
- `548` complete: rename runtime signal compose helpers
- `549` complete: move runtime image cleanup capture behind a signal helper
- `550` complete: remove unused args-based compose signal helpers
- `551` complete: extract runtime data volume/cache planning helpers
- `552` complete: extract runtime data volume IO helpers
- `553` complete: extract runtime data report helpers
- `554` complete: extract runtime data transfer validation helpers
- `555` complete: extract runtime data cache collection helpers
- `556` complete: extract runtime read discovery helpers
- `557` complete: extract runtime read report helpers
- `558` complete: extract runtime write planning helpers
- `559` complete: extract runtime write report helpers
- `560` complete: extract runtime shell exec argument helpers
- `561` complete: close manager-backed runtime read/write/shell

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

Final forbidden-call checks:

- `compose_args(` in `crates/effigy-runtime/src`: only
  `crates/effigy-runtime/src/container_manager.rs`
- `run_docker_capture`, `resolve_compose_backend`, `ComposeBackend` in
  `crates/effigy-runtime/src`: no matches
- runner-local runtime binary construction in `crates/effigy-runtime/src`:
  no `Command::new("docker"|"colima"|"nerdctl")` matches
- runtime file sizes after closeout:
  - `data.rs`: 492 lines
  - `read.rs`: 429 lines
  - `write.rs`: 275 lines
  - `shell.rs`: 249 lines
  - split helper modules: all under 500 lines

First implementation slice:

Rename runtime signal helpers away from Docker-specific names and migrate runner
callers to manager-plan-compatible names while preserving behavior.

## Exit Condition

This lane closes when runtime read/write/shell/data helpers no longer rely on
old compose/process command construction outside named manager adapter seams,
operation reports remain compatible, and focused runtime tests pass.

## Next Task

Card
[`562-scaffold-cli-parser-modularisation-lane.md`](./batch-cards/562-scaffold-cli-parser-modularisation-lane.md).
