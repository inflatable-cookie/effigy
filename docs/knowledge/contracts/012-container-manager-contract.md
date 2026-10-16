# 012 - Container Manager Contract

Owner: Platform
Created: 2026-05-05
Last Updated: 2026-10-16

## Purpose

Effigy container work must route through one manager facade instead of
caller-local Docker, Colima, or nerdctl branching.

The manager owns backend selection, backend capability reporting, operation
shape, and interrupt-aware closeout. Runner code may describe the operation it
needs, but it must not know the process details for Docker Compose or
Colima/nerdctl.

## Manager Owner

The canonical facade crate is `effigy_containers`.

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

Container command intent is planned inside `effigy-containers`. Backend work
then routes through `effigy-containers` and, where compatibility still
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
`effigy-containers`/`effigy-runtime` adapter for container operations.

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
- runner command code takes backend correlation from compose invocation plans
  (`detect_backend` with policy/env detection). `operation_report` /
  `selected_backend` name the first registered backend when the request has
  no override; they are not the compose-backend surface
- drift allowances for named adapter boundaries and private fixtures stay
  path-scoped in `scripts/check-runtime-container-drift.rhai` and are owned by
  [architecture 022](../architecture/022-runtime-architecture-sanity-audit.md)
  and [contract 015](015-runtime-operation-pipeline-contract.md)

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

```sh
effigy qa:architecture:runtime-container-drift
```

The command should pass with only documented, path-scoped allowances.

## Detached reconciliation and capture

Detached Compose launch uses null stdout/stderr sinks so service children cannot
inherit an Effigy capture pipe. Backend failures retain phase/exit classification;
service output is available through the existing logs command. Finite inventory
capture owns its client group, drains nonblocking streams on the supervising
thread, caps retention at 1 MiB per stream and requires both child exit and pipe
EOF before success. Overflow or incomplete capture refuses. Direct-client reap
is not remote container cancellation proof. No detached reader threads remain.

The runner retains an admitted source/context marker across detached success.
Warm/partial reconciliation does not authorize Compose up, primary restart or
cleanup from an unavailable probe. Fresh full-ID/project/service/oneoff/scope/
checkout and published-port inspection gates exposure. Source changes and a
missing primary refuse until explicit scope stop. See contract 005 and guide 063
for command/result ownership and native limits.

### Linux workspace artifact builder execution

The maintained artifact task has a narrow execution-only recovery operation for
an owned running source-bound builder. It does not readmit changed Compose
inputs or grant general application recovery authority. The
[backend compatibility contract](006-compose-backend-compatibility.md) owns its
identity, bind, locking, deadline and refusal rules.
