# 012 - Container Manager Contract

Status: Active
Owner: Platform
Created: 2026-05-05
Last Updated: 2026-05-07

## Purpose

Effigy container work must route through one manager facade instead of
caller-local Docker, Colima, or nerdctl branching.

The manager owns backend selection, backend capability reporting, operation
shape, and interrupt-aware closeout. Runner code may describe the operation it
needs, but it must not know the process details for Docker Compose or
Colima/nerdctl.

## Manager Owner

The canonical facade crate is `effigy_container_manager`.

It owns:

- `ContainerManager`
- `ContainerBackend`
- `ContainerBackendRegistry`
- `BackendId`
- typed operation requests
- typed runtime state
- typed operation reports
- interrupt policy for attached sessions

Static registration is enough for this round. Dynamic plugin loading is out of
scope.

Container command intent is planned by `effigy-container-ops`. Backend work
then routes through `effigy-container-manager` and, where compatibility still
requires it, `effigy-runtime` adapter helpers. The cross-pipeline boundary is
defined in `015-runtime-operation-pipeline-contract.md`.

## Backend Rules

Supported first backends:

- Docker Compose: `docker-compose`
- Colima with nerdctl: `colima-nerdctl`

Backend-specific behavior must stay behind `ContainerBackend`.

Backend-owned details include:

- compose invocation construction
- container/service id and name resolution
- exec, copy, logs, status, stats, up, and down command shape
- Colima repair and retry hooks
- attached-session signal handling and closeout

## Runner Rules

Runner code must call `ContainerManager` or an approved
`effigy-container-ops`/`effigy-runtime` adapter for container operations.

Runner code must not:

- call `resolve_compose_backend()` after migration
- construct local `docker`, `colima`, or `nerdctl` commands
- duplicate backend selection with env or manifest matches
- own Ctrl+C shutdown policy for attached container sessions

Temporary wrappers are allowed only while a card is actively migrating callers
or when they are named adapter boundaries in the package map and drift guard.

## Shipped Migration State

`g03.031` shipped the manager facade and moved runner-level backend selection
behind it. `g04` added typed operation plans and manager-backed runtime
adapters.

Shipped manager-owned surfaces:

- backend detection and explicit override parsing
- Docker Compose and Colima/nerdctl backend ids
- compose process invocation wrapping
- raw runtime process invocation wrapping for copy, volume, and image commands
- internal lifecycle operation reports
- runner-level exec/copy/data/lifecycle backend selection
- operation request/plan surfaces for lifecycle, read, exec/shell, data, and
  cache work

Remaining compatibility boundary:

- `effigy-containers::compose` keeps `ComposeBackend` and
  `resolve_compose_backend()` as temporary lower-level compatibility wrappers
- `effigy-containers::exec` and `effigy-containers::colima` still contain
  backend-local implementation details for Colima repair, runtime probing, and
  direct exec helpers
- those wrappers must not leak back into runner command code
- drift allowances for legacy runner/runtime callers must stay documented in
  `scripts/check-runtime-container-drift.sh` and the active closeout lane

## Operation Reports

Internal operation reports must include at least:

- backend id
- policy name
- repo root
- action
- cleanup result, when cleanup ran or was considered

Reports are internal first. Public CLI JSON shape is unchanged unless a later
card deliberately promotes a report schema.

## Drift Triggers

Update this contract when Effigy changes:

- supported backend ids
- backend capability boundaries
- interrupt or shutdown policy
- container operation report fields
- public CLI exposure of manager reports

## Validation Direction

Minimum proof:

- backend selection honors an explicit override
- Docker Compose invocation shape is stable
- Colima/nerdctl invocation shape is stable
- attached interrupt path runs manager-owned cleanup
- copy, exec, status, logs, and stats route through the manager
- runner drift guards reject caller-local backend branching
- operation plans expose operation kind, side-effect class, and safety policy

Lightweight drift check:

```bash
bash scripts/check-runtime-container-drift.sh
```

The command should pass with only documented, path-scoped allowances.
