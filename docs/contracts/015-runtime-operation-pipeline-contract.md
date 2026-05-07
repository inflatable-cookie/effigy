# 015 - Runtime Operation Pipeline Contract

Status: Active
Owner: Platform
Created: 2026-05-07

## Purpose

This contract defines the `g04` runtime/container architecture after the
modularisation sweep.

Effigy runtime behavior must be built from typed request, plan, report, and
adapter seams. Runner modules may parse CLI input, call the right pipeline, and
render output. They must not re-own backend selection, cwd/root discovery,
container routing, DB command rendering, or artifact handoff locally.

## Pipeline Families

Effigy has four runtime operation pipeline families.

| Pipeline | Owner | Contract responsibility |
| --- | --- | --- |
| Execution pipeline | `effigy-execution` plus runner dispatch adapters | Task and command request construction, surface identity, runtime policy, output mode, environment plan, and resolved route. |
| Runtime activation pipeline | `effigy-runtime-plan` plus `src/runner/container_runtime_prep/*` side-effect stages | Runtime prep identity, readiness, gateway/alias reconciliation, lease policy, and activation report shape. |
| Container operation pipeline | `effigy-container-ops`, `effigy-container-manager`, `effigy-runtime` adapters | Lifecycle, read, exec/shell, data/cache operation intent, safety policy, backend invocation boundary, and operation reports. |
| Artifact/data pipeline | `effigy-data` and `effigy-artifacts` plus runner command adapters | DB target resolution, seed/dump source and destination normalization, database command plans, OCI/local artifact staging, capture, and apply handoff. |

These pipelines are composition seams, not dynamic plugins. Adding a feature
should usually add a request/plan/adapter path, not another caller-local branch.

## Runner Boundary

Runner modules are command-surface shells.

They may:

- receive parsed CLI or embedded command input
- select the correct pipeline entrypoint
- pass captured runtime context and manifest-derived inputs into that pipeline
- run approved side-effect adapters
- render text, JSON, prompts, and operator diagnostics

They must not:

- call `std::env::current_dir()` for new runtime targeting code
- choose Docker, Colima, or nerdctl behavior locally
- construct `docker`, `colima`, or `nerdctl` commands outside approved adapter
  modules
- call compose helpers directly from new runner command logic
- decide host-versus-container execution by probing environment state after
  request construction
- render DB seed/dump shell commands inline when `effigy-data` can render a
  typed command plan
- bypass artifact staging/capture planning for `oci://` seed or dump flows

Compatibility wrappers may remain temporarily, but they must be named as drift
allowances or adapter boundaries.

## Execution Pipeline Rules

Task and command execution starts from `TaskExecutionRequestBuilder`.

Required inputs:

- captured `EffigyRuntimeContext`
- execution intent
- execution surface
- output mode
- runtime policy
- environment plan
- cleanup and handoff policy where applicable

The resolved plan owns host, container, or local-container-handoff route
selection. Rhai `exec::run(...)`, direct CLI, deferral, bootstrap, run-array,
demo re-entry, and managed task dispatch must converge on equivalent plans for
equivalent inputs.

## Runtime Activation Rules

Runtime activation planning belongs to `effigy-runtime-plan`.

Activation plans must carry:

- repo root
- policy name
- selected container name when one exists
- repo override
- readiness expectations
- alias/gateway expectations
- lease refresh policy

Side effects belong behind named runner/runtime stages. A caller may request
activation, but it must not locally rebuild startup, readiness, alias,
gateway, or lease behavior.

## Container Operation Rules

Container operation planning belongs to `effigy-container-ops`.

Operation plans must carry:

- repo root
- policy/container identity
- operation kind
- side-effect class
- safety or confirmation policy for destructive operations
- backend/manager boundary where backend work is needed

Backend behavior belongs behind `ContainerManager` and runtime adapters. Runner
command code should not branch on Docker Compose versus Colima/nerdctl.

## Artifact/Data Rules

Data planning belongs to `effigy-data`.

Artifact transport, staging, apply, and capture belong to `effigy-artifacts`.

Seed and dump surfaces must treat local files and `oci://` refs as typed
sources or destinations. Env vars may exist as compatibility shims, but source
selection and handoff intent must be explicit request data.

Database command rendering belongs behind typed database command plans. Runner
data or bootstrap code should not invent its own Postgres/MariaDB dump/import
command strings.

## Rhai Rules

Rhai runtime-sensitive helpers must route through the same pipeline families as
CLI surfaces.

Required behavior:

- Rhai can inspect captured runtime context
- Rhai `exec::run(..., #{ run_in: "container", ... })` uses the execution
  request builder
- `stdin_file`, cwd, env, container, and service options travel through typed
  execution environment and route plans
- first-party scripts should prefer `exec::run` for container commands that
  depend on host/container context
- direct `container::*` callbacks remain compatibility surfaces until fully
  migrated behind container operation requests

## Drift Guards

The lightweight drift guard is:

```sh
bash scripts/check-runtime-container-drift.sh
```

It checks for new direct uses of cwd discovery, backend branching, raw
Docker/Colima/nerdctl commands, compose helper calls, legacy capture helpers,
and Rhai container-sensitive bypasses outside allowlisted debt.

Allowances must stay path-scoped and documented in the active strict lane or
the follow-up contract/roadmap that owns their removal.

## Proof Expectations

Minimum proof areas:

- direct task host and container routes
- inside-container handoff route
- bootstrap task dispatch target preservation
- Rhai `exec::run(... run_in=container ...)` with `stdin_file`
- run-array embedded dispatch
- demo task re-entry
- managed dev activation identity and lease policy
- `effigy exec` activation identity and handoff transport boundary
- workspace seeded/bootstrap handoff cleanup and repo override propagation
- container data seed from local SQL and OCI artifact
- container data dump to local SQL and OCI artifact destination
- `container reset --keep-data` persistent-data safety
- Ctrl+C or attached-session closeout through manager-owned cleanup

Proofs should prefer pure request/plan tests and fake side-effect adapters.
Live container boot is reserved for focused compatibility checks.

## Drift Triggers

Update this contract when Effigy changes:

- pipeline crate ownership
- request, plan, report, or adapter boundaries
- runner drift guard rules or allowlist policy
- Rhai host/container execution behavior
- seed/dump artifact handoff semantics
- manager-backed operation surfaces
- public JSON exposure of operation reports
