# 015 - Runtime Operation Pipeline Contract

Owner: Platform
Created: 2026-05-07
Last Updated: 2026-10-06

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
| Container operation pipeline | `effigy-containers` plus `effigy-runtime` adapters | Lifecycle, read, exec/shell, data/cache operation intent, safety policy, backend invocation boundary, and operation reports. |
| Artifact/data pipeline | `effigy-data` and `effigy_state::artifact` plus runner command adapters | DB target resolution, seed/dump source and destination normalization, database command plans, OCI/local artifact staging, capture, and apply handoff. |

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

Task and command execution starts from `TaskExecutionRequestBuilder`. Its
required inputs are owned by contract `013-task-execution-request-contract.md`.

The resolved plan owns host, container, or local-container-handoff route
selection. Rhai `exec::run(...)`, direct CLI, deferral, bootstrap, run-array,
and managed task dispatch must converge on equivalent plans for
equivalent inputs.

## Runtime Activation Rules

Runtime activation planning belongs to `effigy-runtime-plan`.

Activation plans carry every fact the side-effect stages need, so a stage never
re-derives identity, readiness, alias, gateway or lease policy. The fields are
owned by `effigy-runtime-plan`.

Side effects belong behind named runner/runtime stages. A caller may request
activation, but it must not locally rebuild startup, readiness, alias,
gateway, or lease behavior.

## Container Operation Rules

Destructive operations require explicit consent before runtime effects.

Backend behavior belongs behind `ContainerManager` and runtime adapters. Runner
command code should not branch on Docker Compose versus Colima/nerdctl.

## Artifact/Data Rules

Data planning belongs to `effigy-data`.

Artifact transport, staging, apply, and capture belong to
`effigy_state::artifact`.

Seed and dump surfaces must treat local files and `oci://` refs as typed
sources or destinations. Env vars may exist as compatibility shims, but source
selection and handoff intent must be explicit request data.

Database command rendering belongs behind typed database command plans. Runner
data or bootstrap code should not invent its own Postgres/MariaDB dump/import
command strings.

## Rhai Rules

Rhai runtime-sensitive helpers must route through the same pipeline families as
CLI surfaces.

Rhai `exec::run(...)` uses the execution request builder, and its environment
and route options travel as typed plan data. Direct `container::*` callbacks
remain compatibility surfaces until they move behind shared execution requests.

## Drift Guards

The lightweight drift guard is:

```sh
effigy qa:architecture:runtime-container-drift
```

It checks the runner boundary rules above; a match is allowed only as a named
adapter boundary or migration debt.

Allowances must stay path-scoped and documented here and in the
[package map](../architecture/010-package-map.md).
A private fixture file may be allowed; a production file that also holds
fixtures may not. Positive and negative controls:
`effigy test:architecture:runtime-container-drift-controls`.

The guard is part of normal validation through `qa:gates`, `qa:ci`, and
`prepush:ci`. Day-to-day `qa` remains focused on tests, docs, and JSON unless
that task is deliberately promoted to gates later.

Suppression rules:

- prefer moving code behind the proper request/plan/adapter seam
- allow a match only when the file is an intentional adapter boundary or a
  named migration debt item
- keep allowlist comments path-scoped in the guard script
- include the owning roadmap/card in the comment when the allowance is
  migration debt
- do not add broad regex suppressions for whole directories unless that
  directory is itself the adapter boundary

## Proof Expectations

Proof areas are owned by `009-execution-surface-convergence.md`, Validation
direction. Proofs prefer pure request and plan tests with fake side-effect
adapters; live container boot is reserved for focused compatibility checks.

## Drift Triggers

Update this contract when Effigy changes:

- pipeline crate ownership
- request, plan, report, or adapter boundaries
- runner drift guard rules or allowlist policy
- Rhai host/container execution behavior
- seed/dump artifact handoff semantics
- manager-backed operation surfaces
- public JSON exposure of operation reports
