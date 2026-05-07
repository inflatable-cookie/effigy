# 005 - Container Runtime Contract

Status: Active
Owner: Platform
Last Updated: 2026-05-07

This contract defines the required runtime guarantees for container-backed
task execution in Effigy.

It covers the execution surfaces that depend on a running local container
environment, not production deployment export.

## Purpose

Effigy now has several public surfaces that rely on the same local runtime:

- managed `dev` tasks
- standard routed tasks with `run_in = "container"`
- workspace shell handoff
- bootstrap flows that dispatch container-backed tasks

Those surfaces must not drift in semantics based on whether the command was
launched through a TUI, a plain shell, or a one-shot bootstrap path.

This contract exists to keep one explicit runtime guarantee across those
surfaces.

## Runtime-backed execution surfaces

The contract applies to any Effigy surface that:

- resolves a task or shell into a container-backed workspace
- auto-starts or reuses a local runtime
- dispatches work through container exec or container-local Effigy handoff

The first covered surfaces are:

- managed `dev` flows
- standard routed task execution
- `effigy workspace`
- bootstrap-driven container task execution
- Rhai container-targeted execution helpers

Direct operator use of raw compose commands is outside this contract.

## Contract goals

When Effigy routes work into a container-backed runtime, it should guarantee:

- one clear runtime-prep phase before exec or handoff
- one stable container-handoff marker contract
- one honest alias-resolution contract inside the execution target
- one explicit fallback boundary when the compose backend does not provide the
  expected runtime behavior directly

## Source of truth

The runtime contract derives from:

- the captured `EffigyRuntimeContext`
- the effective manifest
- the resolved container policy
- the generated compose/runtime metadata
- the live runtime state when readiness or published-port inspection is needed

It must not depend on whether a user happened to start the runtime previously
through `dev`, `workspace`, `container up`, or `bootstrap`.

It must also not depend on caller-local cwd, env, Docker, Colima, or nerdctl
probing in runner command modules.

## Runtime prep contract

Before Effigy dispatches work into a container-backed target, it must treat
runtime preparation as a required phase, not an optional side effect.

The runtime-prep phase owns:

- ensuring the selected runtime exists and is running
- ensuring required shared services are available
- ensuring the target exec surface is ready for the requested working
  directory and command style
- reconciling runtime guarantees that the compose backend failed to materialize
  directly
- reconciling gateway/runtime exposure needed by the selected execution target
- refreshing non-shell task leases when Effigy owns warm-runtime reuse

Surface-specific presentation may differ. The prep contract must not.

Runtime prep must consume captured context facts and typed execution policy. It
must not rediscover invocation cwd or handoff state after request construction.

Runtime activation planning belongs to `effigy-runtime-plan`. The runner
runtime-prep modules are side-effect adapters for that plan: they may start
the runtime, perform readiness checks, reconcile aliases/routes, and refresh
leases, but they must not invent a separate activation model.

## Handoff contract

Effigy has two valid execution modes inside a running container:

- container-local Effigy handoff
- raw container exec

The decision may depend on container capabilities, but the recursion guard
must be one shared contract.

The runtime handoff marker is:

- env var: `EFFIGY_INTERNAL_CONTAINER_HANDOFF=1`

Meaning:

- when present inside the container, Effigy is already executing inside a
  container handoff
- routing must not recurse back into container dispatch
- `stay_in_shell` and related handoff-only behavior must treat that state
  consistently across managed and standard surfaces

The marker name and meaning are product contract, not incidental plumbing.
`EffigyRuntimeContext` captures marker presence at process entry; downstream
runtime code should consume that captured state when a context is available.

## Container manager contract

Runner-facing container operations must route through `ContainerManager`.

This covers:

- backend selection
- compose invocation shape
- container exec and shell operation shape
- copy, logs, status, stats, up, and down operation shape
- backend-owned repair or retry behavior
- attached-session interrupt closeout
- internal operation reports

Backend-specific Docker Compose and Colima/nerdctl behavior belongs behind the
manager facade. Runner command modules may request a container operation, but
must not branch on backend internals or construct `docker`, `colima`, or
`nerdctl` process commands locally.

The detailed manager contract lives in
`012-container-manager-contract.md`. The cross-pipeline runner boundary lives
in `015-runtime-operation-pipeline-contract.md`.

## Activation ownership

Effigy has two valid ownership models for container-backed local work:

- public shell/session ownership
- non-shell task activation ownership

These must not drift by caller path.

### Public shell/session ownership

This covers:

- `effigy dev`
- `effigy workspace`
- `stay_in_shell` handoff flows

Contract:

- Effigy prepares the runtime for interactive access
- gateway/public route exposure must be reconciled before the shell opens
- session shutdown ownership depends on whether Effigy completed runtime
  readiness for that shell

This is not lease-managed task activation.

### Non-shell task activation ownership

This covers:

- standard routed tasks with `run_in = "container"`
- deferred requests with `[defer].run_in = "container"`
- bootstrap `run` steps that dispatch into a container without opening a shell
- Rhai `exec::run(...)` requests with container runtime policy

Contract:

- the runtime-prep phase runs before user command dispatch
- if Effigy auto-started the runtime, or the runtime was already under an
  active task lease, Effigy refreshes the host-container lease
- default lease timeout is 5 minutes unless configured otherwise
- lease reuse must not depend on whether the request came from deferral or
  explicit task routing

This is the required shared contract for warm non-shell container reuse.
Task-shaped requests should reach this contract through
`TaskExecutionRequestBuilder` and a resolved execution plan.

## Alias contract

Effigy owns two related but distinct alias surfaces:

- host-visible local domains and service names
- container-local service resolution needed by container-backed tasks

Those must not be conflated.

### Host-visible alias contract

Effigy gateway and runtime orchestration own the host-visible `.test` naming
surface.

That includes:

- HTTP route domains
- TCP service aliases derived from shipped service catalogs
- project-owned aliases
- shared-service aliases where several project-facing names collapse onto one
  shared backing-service identity

This surface is validated against live runtime port data where needed.

### Container-local alias contract

Container-backed tasks must be able to resolve the service aliases they
depend on from inside the container execution target.

The first guaranteed alias class is:

- TCP backing-service aliases such as `mysql.<site>.legacy.test`

The guarantee applies inside any container target Effigy uses for:

- workspace handoff
- routed task execution
- container-local Effigy handoff

It does not promise that every arbitrary compose service in the runtime sees
those aliases automatically. The guarantee is scoped to Effigy-owned execution
targets.

### Alias source rules

Container-local aliases must derive from the same effective service-alias
model as the host-visible TCP alias surface.

That means:

- project-owned aliases keep their declared domain
- shared-service aliases may resolve through one shared backing-service host
- explicit route/alias precedence must remain stable

Effigy must not invent a second alias naming scheme for container-local
resolution.

## Fallback ownership

Compose backends do not all materialize runtime features equally.

When a supported backend fails to provide the required alias behavior or other
covered runtime guarantees directly, Effigy may repair that gap during the
runtime-prep phase.

That fallback is legitimate product behavior when:

- the repaired state preserves the documented runtime contract
- the repair derives from the same effective model as the non-fallback path
- the repair is scoped to Effigy-owned execution targets

The current expected fallback class is:

- container-local TCP alias reconciliation when the backend does not expose
  service aliases reliably inside the execution target

Fallback ownership must stay explicit in code and docs. It is not acceptable
for one surface to rely on backend luck while another surface performs the
repair.

The same rule applies to task activation:

- one non-shell caller path must not refresh leases while another caller path
  tears the runtime down immediately
- one caller path must not reconcile gateway/public routes while another
  caller path silently skips them for the same container policy

## Failure semantics

If runtime preparation cannot establish the contract required for the selected
execution target, Effigy should fail before dispatching the user command.

Examples:

- target service cannot satisfy exec readiness
- required alias reconciliation cannot resolve its backing service
- runtime policy and live runtime state disagree in a way Effigy cannot repair

Failure should report the runtime guarantee that could not be established, not
just the downstream task failure it would have caused.

## Validation direction

This contract should be covered by targeted compatibility tests rather than
large generic runtime smoke tests.

The minimum proof set should cover:

- managed execution and standard execution reaching the same handoff semantics
- workspace handoff and bootstrap-backed task execution sharing the same alias
  guarantees
- runtime prep repairing backend-sensitive gaps before the user command runs
- compatibility behavior on the supported Colima + `nerdctl compose` path
- runner container commands routing through `ContainerManager`
- container-targeted execution plans consuming captured runtime context instead
  of caller-local cwd/env probes
- runtime activation plans preserving repo root, repo override, policy name,
  container identity, and lease policy across `effigy exec`, workspace, and
  managed surfaces

## Drift triggers

Update this contract when Effigy changes:

- the handoff marker name or meaning
- which execution targets receive container-local alias guarantees
- the boundary between gateway-owned and runtime-owned alias behavior
- the runtime-prep steps required before exec or handoff
- the supported backend fallback model
- runner-facing container manager operation ownership
- runtime context facts used by container-backed execution
- runtime activation request/plan/report fields

## Next Task

Use this contract with `011-runtime-context-contract.md`,
`012-container-manager-contract.md`, and
`013-task-execution-request-contract.md`, plus
`015-runtime-operation-pipeline-contract.md`, as the durable authority set for
container-backed local runtime behavior.
