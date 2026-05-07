# 376 - Design Rhai Runtime Context And Execution Helper

Lane: [`036-universal-runtime-context-and-path-authority-strict-lane.md`](../036-universal-runtime-context-and-path-authority-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05

## Goal

Make the DecodeLabs mysql seed failure a first-class design target for the
runtime/context/execution modularisation work.

## Scope

- define the Rhai read-only runtime context helper backed by
  `EffigyRuntimeContext`
- define the Rhai execution helper backed by `TaskExecutionRequestBuilder`
- specify options for `run_in`, `container`, `service`, `cwd`, `env`, and
  `stdin_file`
- define inside-container behavior so container handoff does not recurse
- inventory first-party Rhai scripts that should move from
  `process::run(...)` or `container::exec(...)` to the execution helper
- include the DecodeLabs mysql seed script as the reference migration

## Exit Condition

This card is complete when the Rhai API contract is clear enough for
`g03.032`, first-party script migrations are listed, and the DecodeLabs seed
proof is represented in the `g03.034` matrix.

## Design

Rhai gets two new planned modules:

- `runtime::context()`
- `exec::run(command, options)`

`runtime::context()` returns a read-only map backed by `EffigyRuntimeContext`.
Required fields:

- `invocation_cwd`
- `command_root`
- `repo_override`
- `invocation_mode`
- `inside_container_handoff`
- `host.os`
- `host.arch`
- `host.no_color`
- `host.ci`

`exec::run(...)` is backed by `TaskExecutionRequestBuilder`. It accepts:

- `run_in`: `"host"`, `"container"`, or `"either"`
- `container`
- `service`
- `cwd`
- `env`
- `stdin_file`
- `output`

Initial output mode is capture-only. Stream and tee modes can be added once the
execution builder owns interactive policy.

Routing rules:

- host intent maps to host process execution
- container intent maps to container manager execution from host mode
- container intent in container handoff mode runs directly when the active
  handoff matches the requested target
- `cwd` and `stdin_file` resolve from the captured command root unless absolute
- scripts must not branch between `process::run(...)` and
  `container::exec(...)` for a command whose target can be expressed as
  `run_in`

First-party migration list:

- DecodeLabs `seed-latest-db-dump.rhai`
- Underlay `error-reporting.rhai`
- any shipped script using `container::exec(...)` for service-local commands
- any shipped script using `process::run(...)` to compensate for unclear
  host/container context

DecodeLabs mysql seed target:

```rhai
let result = exec::run(
    mysql_args + [database],
    #{
        run_in: "container",
        container: container_name,
        service: "db",
        stdin_file: staged_path,
    },
);
```

This is the shape `g03.032` must implement.

## Validation

- docs link check for the updated Rhai/context surfaces
- no code migration required in this design card

## Next Task

Open the implementation card for exposing `runtime::context()` to Rhai, then
wire `exec::run(...)` when `TaskExecutionRequestBuilder` exists.
