# 012 - Container Manager Contract

Status: Active
Owner: Platform
Created: 2026-05-05

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

Runner code must call `ContainerManager` for container operations.

Runner code must not:

- call `resolve_compose_backend()` after migration
- construct local `docker`, `colima`, or `nerdctl` commands
- duplicate backend selection with env or manifest matches
- own Ctrl+C shutdown policy for attached container sessions

Temporary wrappers are allowed only while a card is actively migrating
callers.

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
