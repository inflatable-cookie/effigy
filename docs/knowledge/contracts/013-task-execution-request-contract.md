# 013 - Task Execution Request Contract

Owner: Platform
Created: 2026-05-05
Last Updated: 2026-10-06

## Purpose

Effigy task execution must start from one typed request model instead of each
caller rebuilding runtime intent, cwd, env, output mode, or container routing
locally.

This contract covers direct task execution, deferral, bootstrap task dispatch,
Rhai execution helpers, run-array re-entry, demo task re-entry, and managed task
flows.

It exists to stop bugs where the caller knows too little about whether Effigy is
running on the host or inside a container and guesses the wrong path or process
surface.

## Request Owner

The canonical request owner is `effigy_execution`.

Public contract types:

- `TaskExecutionRequest`
- `TaskExecutionRequestBuilder`
- `ResolvedTaskExecutionPlan`
- `ExecutionSurface`
- `ExecutionIntent`
- `ExecutionOutputMode`
- `ExecutionRuntimePolicy`
- `ExecutionHandoffPolicy`
- `ExecutionCleanupPolicy`
- `ExecutionEnvironmentPlan`

Runner code may still own command presentation and final dispatch mechanics, but
it must not invent a parallel task-request model.

Runtime activation, container operation, and artifact/data handoff boundaries
are defined by `015-runtime-operation-pipeline-contract.md`. This contract owns
only the task/command execution request and resolved route model.

## Required Inputs

Every task execution request must carry:

- `EffigyRuntimeContext`
- an execution intent: task selector plus args, or raw command argv
- an execution surface label
- output mode
- runtime policy
- handoff policy
- cleanup policy
- environment plan

The runtime context is mandatory. A request without captured context is invalid.
Its optional explicit task-source identity must survive dispatch and preflight;
it never replaces the target root in the same context.

The invocation is mandatory. A request without task selector or command argv is
invalid.

Defaults are allowed only where the product contract has a safe default:

- surface: direct CLI
- output mode: capture
- runtime policy: either
- handoff policy: allow container handoff
- cleanup policy: preserve
- environment plan: empty

## Resolution Rules

Resolution turns `TaskExecutionRequest` into `ResolvedTaskExecutionPlan`.

The plan owns the resolved route:

- `Host`
- `Container`
- `LocalContainerHandoff`

Route selection must derive from the runtime policy and the captured runtime
context.

Rules:

- `run_in = host` resolves to host execution.
- `run_in = container` resolves to container execution on the host.
- `run_in = container` resolves to local container handoff when the captured
  context is already inside Effigy's container handoff.
- `run_in = either` resolves to local container handoff inside container
  handoff, otherwise host execution.
- container and service intent must be preserved on container routes.
- cwd, env overrides, and `stdin_file` belong to `ExecutionEnvironmentPlan` and
  must travel with the plan.
- when runtime context carries an explicit task source, task discovery uses
  that isolated source while the environment CWD and repo target remain the
  independently resolved consumer
- the selected catalog's declared runtime target owns container routing: when
  the selected task's manifest declares `[systems]`, the execution binding
  resolves from that manifest, and container policy loading, runtime
  activation, and container exec use that catalog root. Invocation-scope
  (workspace) ownership never replaces a selected catalog's declared target.
- a selected catalog that declares no runtime target inherits the
  invocation-scope (ancestor) runtime. `run_in = "container"` with no
  resolvable target fails before execution instead of silently running on the
  host or choosing a container by location.

No caller may decide host-versus-container execution by directly probing cwd,
env vars, or process state after a request has been built.

## Surface Rules

Covered surfaces:

- direct CLI
- deferral
- bootstrap
- Rhai
- run-array
- demo
- managed

Surface labels are not routing hints by themselves. They preserve caller
identity for presentation, diagnostics, policy gates, and proof coverage.

Equivalent inputs across surfaces should produce equivalent plans unless a
contracted surface difference is named explicitly.

## Rhai Rules

Rhai scripts must use the execution request surface for context-sensitive
process work.

When a Rhai script needs a command to run in a container, it should submit a
typed request with container runtime policy instead of deciding between
`process::run(...)` and container exec locally.

Required behavior:

- scripts can read the captured runtime context
- scripts can request container execution by intent
- `stdin_file` paths are carried in the environment plan
- inside-container handoff turns container intent into local container handoff
- host execution remains available for explicitly host-scoped commands
- first-party scripts that need context-sensitive container commands should use
  `exec::run(..., #{ run_in: "container", ... })` rather than choosing between
  process and container helpers locally

This is the contract that hardens DecodeLabs mysql seed scripts and similar
bundle-backed scripts against host/container path drift.

## Embedded Dispatch Rules

Embedded callers must build or consume the same request shape as direct CLI
dispatch.

This includes:

- bootstrap `run`
- bootstrap `start`
- run-array builtin command re-entry
- Rhai command re-entry
- demo task re-entry
- deferral replay

Embedded dispatch must preserve the parent or explicit target repo from
`EffigyRuntimeContext`. It must not fall back to process cwd.

## Output Rules

Output mode must be decided once per request.

Valid modes:

- capture
- stream
- tee
- JSON
- interactive

Surface-specific rendering may still differ, but runtime route and environment
resolution must not change because a caller wrapped the output differently.

`--json` is an output format. It must not change execute-versus-plan, and it
must not be treated as a safety flag. Inspecting a selector without running it
is a separate `--plan` operation on the task or draft invocation.

Direct CLI presentation (the framed header) is not task output. For
`effigy <selector>` and `effigy draft <selector>` text runs, that header goes
to stderr so stdout is only the task's own bytes and the process keeps the
task's success or failure status. JSON envelopes stay on stdout and already
suppress the header.

## Cleanup and Handoff Rules

Cleanup and handoff policy belong to the request.

Local booleans in command modules must not re-own:

- recursive container handoff decisions
- stay-in-shell policy
- task lease versus session ownership
- cleanup-on-success or cleanup-always behavior

Those behaviors may still be implemented by runner modules, but the requested
policy must come from the resolved execution plan.

## Public API Boundary

`ResolvedTaskExecutionPlan` is an internal host/container routing contract. It
is not a public CLI payload.

The public non-executing selector plan is a separate schema,
`effigy.task.plan.v1`. `effigy <selector> --plan` and
`effigy draft <selector> --plan` resolve the task, catalog, and command shape
without starting a task process, acquiring locks, activating containers,
resolving env-schema `exec()` values, or writing task-status records. `--plan`
is consumed by Effigy and is not passed through to the task. `--json` only
chooses the envelope. Unresolved selectors do not run deferral, exec aliases,
or other builtins. Builtin `--plan` surfaces such as `effigy test --plan` keep
their own meaning. A remapped `[defer].builtins` name still receives that
command's own `--plan`.

The selector plan exposes the resolved runtime target through an additive
`runtime` section (`target`: `host`, `container`, or `inline-container`, plus
`container`, `service`, and owning `root` when a container target resolves).
It resolves through the same binding path as execution, so a plan cannot
promise host execution for a task whose declared target is a container, and a
missing or ambiguous declared target fails the plan instead of reporting a
host route. `effigy.test.plan.v1` exposes the same resolution per target
(`runtime.target`: `host`, `container`, or `unusable` with `reason`); suites
that carry nested effigy re-entries keep their own routing and are never
wrapped into the target container again.

## Drift Triggers

Update this contract when Effigy changes:

- execution request fields
- execution surfaces
- route selection rules
- selected-catalog runtime-target ownership, the ancestor runtime fallback, or
  container-suite routing through runtime ports
- Rhai execution helper behavior
- embedded task dispatch behavior
- task-source propagation through preflight and nested dispatch
- public `effigy.task.plan.v1` selector-plan fields
- runtime operation pipeline boundaries that change what execution requests
  must carry

## Validation Direction

Minimum proof:

- `cargo test -p effigy-execution`
- direct task, bootstrap task, and Rhai task parity proof
- focused selector-plan proofs that no task process starts
- the existing `--json <selector>` execution envelope
- Rhai container-targeted mysql seed execution with `stdin_file`
- inside-container handoff routes container intent locally
- repo override propagation is identical across embedded callers
- explicit skill source identity survives nested task and Rhai dispatch while
  ordinary task routing remains unchanged
- env overrides merge through one path
- QA-group member requests set `environment.cwd` to the group's resolved root
  so `ExecutionDispatchPlan.effective_cwd` is that repository, not process cwd
- drift checks reject embedded task dispatch that bypasses
  `TaskExecutionRequestBuilder`
- drift checks and Rhai tests reject first-party container-sensitive scripts
  bypassing `exec::run`
