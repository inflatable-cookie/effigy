# 031 - Plugin-Ready Container Manager Facade

Generation: `g03`

Status: Complete
Owner: Platform
Created: 2026-05-05
Completed: 2026-05-05
Depends on: [`030-universal-runtime-context-and-path-authority.md`](./030-universal-runtime-context-and-path-authority.md)

## Goal

Give container operations one manager API and remove caller-local Docker,
Colima, and nerdctl branching.

## Scope

- add `crates/effigy-container-manager`
- define a static backend registry and `ContainerBackend` trait
- implement Docker Compose and Colima/nerdctl backends
- move backend detection and invocation construction behind the manager
- route exec, copy, logs, up/down, status, stats, and interrupt-aware attached
  sessions through the facade

## Non-Goals

- dynamic plugin loading
- adding new public backends beyond Docker and Colima in this round
- public JSON schema changes for operation reports

## Next Task

Roadmap complete. Continue with `g03.033` or choose the next queued roadmap
deliberately.
