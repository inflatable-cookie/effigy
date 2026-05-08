# 444 - Scaffold Runtime Activation Pipeline Lane

Lane: [`045-runtime-activation-pipeline-strict-lane.md`](../045-runtime-activation-pipeline-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Goal

Scaffold the `g04.003` runtime activation implementation lane and select the
first safe activation-planning slice.

## Scope

- inventory runtime activation callers and side-effect stages
- define the first `effigy-runtime-plan` crate boundary
- decide the first implementation slice
- create the next bounded implementation card
- keep runtime/container side effects unchanged

## Non-Goals

- no runtime activation implementation yet
- no container manager migration
- no public CLI behavior changes
- no release work
- no `.github/workflows/` edits

## Exit Condition

This card is complete when `g04.003` has a concrete first implementation card
and the lane/front-door docs point to it.

## Inventory

Primary activation owner:

- `src/runner/container_runtime_prep/mod.rs`

Current activation stages mixed in that file:

- policy validation
- backend validation
- running-state check
- detached container up
- host bind mount prep
- compose up idempotency
- primary service exec readiness probe
- primary service restart/retry
- gateway startup
- gateway route registration
- primary service alias reconciliation
- host container lease refresh

Current activation callers:

- `src/runner/execute/pipeline/standard.rs`
- `src/runner/exec_command/mod.rs`
- `src/runner/db_seed.rs`
- `src/runner/deferral/run.rs`

Related runtime prep callers:

- `src/runner/system_command/workspace_session.rs`
- `src/runner/container_command/lifecycle.rs`
- `src/runner/execute/pipeline/managed.rs`

Known side-effect dependencies that must stay runner-owned until plan types
exist:

- `run_container(...)`
- `run_docker_capture(...)`
- `compose_args(...)`
- `ensure_colima_running(...)`
- `load_container_exec_working_dir(...)`
- `register_gateway_routes_for_container(...)`
- `gateway_up_for_managed_task(...)`
- `refresh_host_container_lease_for_task_activation(...)`
- `reconcile_primary_service_tcp_alias_hosts(...)`

## Decision

First implementation slice: scaffold `crates/effigy-runtime-plan` with pure
activation request, plan, stage, and report types.

Do not migrate `container_runtime_prep` side effects yet. The first crate should
be dependency-light and prove the activation stage vocabulary before runner
starts moving runtime behavior.

## Closeout

Opened the runtime activation lane and selected card `445` as the first
implementation slice.

## Validation

- docs path check for updated spec and roadmap front doors
- `git diff --check`

## Next Task

Start card
[`445-scaffold-effigy-runtime-plan-crate.md`](./445-scaffold-effigy-runtime-plan-crate.md).
