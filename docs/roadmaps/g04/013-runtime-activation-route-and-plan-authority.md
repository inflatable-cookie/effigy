# 013 - Runtime Activation Route And Plan Authority

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-07
Depends on: [`012-runtime-pipeline-integration-audit-and-debt-map.md`](./012-runtime-pipeline-integration-audit-and-debt-map.md)

## Goal

Make `effigy-runtime-plan` the honest activation-plan authority instead of a
thin identity object that callers duplicate.

## Scope

- add route selection to `RuntimeActivationRequest`
- set `RuntimeActivationRoute` from exec, standard task, managed task,
  deferral, DB seed, workspace, bootstrap, and Rhai surfaces
- centralize repeated repo-root, repo-override, policy-name, container-name,
  and lease-policy activation-plan construction
- make activation reports distinguish task, exec, workspace, bootstrap, Rhai,
  and managed paths where appropriate
- replace local helper constructors in runner modules with one shared builder
  or adapter
- keep side effects in runner/runtime prep stage modules

## Migration Targets

- `src/runner/container_runtime_prep/mod.rs`
- `src/runner/exec_command/mod.rs`
- `src/runner/execute/pipeline/standard.rs`
- `src/runner/execute/pipeline/managed.rs`
- `src/runner/deferral/run.rs`
- `src/runner/db_seed.rs`
- `src/runner/script_command/mod.rs`
- `src/runner/system_command/workspace/*`

## Acceptance Criteria

- activation route is never silently defaulted to `Task` for non-task surfaces
- duplicated activation-plan construction is removed or reduced to thin
  adapters
- focused tests prove route, repo override, policy name, container name, and
  lease policy for exec, workspace, managed, deferral, DB seed, and Rhai paths
- contracts and package map remain accurate

## Validation

- `cargo test -p effigy-runtime-plan`
- targeted runner tests for touched activation callers
- `bash scripts/check-runtime-container-drift.sh`
- `git diff --check`

## Next Task

Continue with
[`g04.014`](./014-data-seed-dump-plan-consumption.md).
