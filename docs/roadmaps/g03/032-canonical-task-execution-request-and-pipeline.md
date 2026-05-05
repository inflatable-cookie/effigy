# 032 - Canonical Task Execution Request And Pipeline

Generation: `g03`

Status: Active
Owner: Platform
Created: 2026-05-05
Depends on: [`030-universal-runtime-context-and-path-authority.md`](./030-universal-runtime-context-and-path-authority.md)

## Goal

Make task execution explicit and reusable from direct CLI, deferral, bootstrap,
Rhai, run-array, demo, and managed flows.

## Scope

- add `crates/effigy-execution`
- define `TaskExecutionRequestBuilder`
- move task preflight discovery into one request path
- make runner execution consume a resolved request/plan
- replace ad hoc task invocation construction in embedded callers
- expose the request builder to Rhai as the canonical command execution helper
- migrate first-party Rhai scripts away from direct host/container branching
  where `run_in` intent is enough

## Rhai Requirement

Rhai must get two typed surfaces from this milestone:

- `runtime::context()` or equivalent read-only context map backed by
  `EffigyRuntimeContext`
- an execution helper backed by `TaskExecutionRequestBuilder`, with options such
  as `run_in = "container"`, `container`, `service`, `stdin_file`, `cwd`, and
  `env`

Target helper names:

- `runtime::context()`
- `exec::run(command, options)`

`exec::run(...)` must return the normal process-like output map plus a route
summary. It must accept `run_in`, `container`, `service`, `stdin_file`, `cwd`,
and `env` options. `stdin_file` and `cwd` resolve through the captured runtime
context, not script-local cwd guessing.

The helper owns the host/container decision. First-party scripts must not have
to choose between `process::run(...)` and `container::exec(...)` when the desired
execution target is expressible as a request.

The DecodeLabs mysql seed helper is the reference bug:

- mysql must execute inside the database service container
- SQL dump paths must be interpreted from the captured repo/runtime context
- inside-container handoff must avoid recursive container exec
- the script should express `run_in = "container"` plus service/stdin intent,
  not reconstruct host and container paths locally

First-party migration list:

- DecodeLabs `seed-latest-db-dump.rhai`
- Underlay `error-reporting.rhai`
- any shipped script that shells into a service with `container::exec(...)`
- any shipped script that uses `process::run(...)` because it is uncertain
  whether the current process is host-side or container-side

## Non-Goals

- public CLI changes by default
- container backend extraction beyond what `g03.031` owns

## Next Task

Implement card `378` to scaffold `crates/effigy-execution`.
