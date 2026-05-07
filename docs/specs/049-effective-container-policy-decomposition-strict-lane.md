# 049 - Effective Container Policy Decomposition Strict Lane

Roadmap: [`g04.007`](../roadmaps/g04/007-effective-container-policy-decomposition.md)

Status: Complete
Owner: Platform
Created: 2026-05-07

## Purpose

Split `effigy-containers` into smaller policy, workspace, runtime, and backend
ownership modules while keeping public exports stable.

## Hard Boundaries

- preserve public `effigy-containers` exports during migration
- prefer mechanical module extraction before behavior changes
- no broad formatting churn
- no release work
- no `.github/workflows/` edits

## Current Ready Card

None. This lane is complete.

## Execution Chain

- `528` complete: close Rhai host API split and callback purity
- `529` complete: scaffold effective container policy decomposition lane
- `530` complete: extract effective container policy model module
- `531` complete: extract effective container policy project module
- `532` complete: extract effective container policy validation module
- `533` complete: extract inline workspace policy module
- `534` complete: extract runtime DNS policy module
- `535` complete: extract generated compose eject module
- `536` complete: extract container policy load module
- `537` complete: extract workspace host-integration module
- `538` complete: extract workspace library mounts module
- `539` complete: extract workspace isolation mounts module
- `540` complete: extract workspace compose rewrite module
- `541` complete: extract generated compose source module
- `542` complete: extract container exec implementation module
- `543` complete: extract container exec parse module
- `544` complete: extract container exec process module
- `545` complete: extract container exec Colima runtime module
- `546` complete: close effective container policy decomposition

## Decomposition Inventory

Large ownership-mixed files:

- `crates/effigy-containers/src/lib.rs`: 1563 lines
- `crates/effigy-containers/src/workspace.rs`: 1541 lines
- `crates/effigy-containers/src/policy_support.rs`: 1306 lines
- `crates/effigy-containers/src/exec.rs`: 1684 lines

First module seams:

- `policy/model.rs`: effective policy structs, host process structs, DNS route
  structs, service aliases, shared service bindings, attach/compose-source
  enums, policy errors, eject result, driver labels
- `policy/load.rs`: `load_container_policy`, `load_all_container_policies`,
  manifest/catalog resolution, library mount resolution
- `policy/project.rs`: project-name defaulting, sanitation, uniqueness, fresh
  bootstrap suffix
- `policy/validation.rs`: host path checks, mount budget checks, backend runtime
  validation
- `policy/inline_workspace.rs`: inline workspace policy rendering and exec
  working directory resolution
- `workspace/mounts.rs`: workspace runtime mounts, extra mounts, library mounts
- `workspace/host_integration.rs`: SSH, known_hosts, composer, mkcert, agent
  mounts
- `workspace/isolation.rs`: isolation/adopted repo mounts
- `workspace/compose_rewrite.rs`: service volume rewrite, named volumes,
  compose-relative path normalization
- `runtime/dns.rs`: runtime DNS override materialization and rendering
- `runtime/eject.rs`: generated compose eject support

First implementation slice:

Extract `policy/model.rs` first. It is mostly type movement and public export
stabilization, so it gives later load/validation/project splits a clean home
without changing behavior.

## Exit Condition

This lane closes when `effigy-containers` has clear module owners for policy,
workspace, runtime, and backend helpers, `lib.rs` is mostly exports/top-level
orchestration, and focused container tests pass.

## Next Task

Open the manager-backed runtime read/write/shell lane for `g04.008`.
