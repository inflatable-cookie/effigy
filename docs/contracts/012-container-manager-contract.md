# 012 - Container Manager Contract

Status: Active
Owner: Platform
Created: 2026-05-05
Last Updated: 2026-08-01

## Purpose

Effigy container work must route through one manager facade instead of
caller-local Docker, Colima, or nerdctl branching.

The manager owns backend selection, backend capability reporting, operation
shape, and interrupt-aware closeout. Runner code may describe the operation it
needs, but it must not know the process details for Docker Compose,
Colima/nerdctl, or any future native runtime.

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

- runtime invocation construction, including Compose only where the backend
  uses Compose
- container/service id and name resolution
- exec, copy, logs, status, stats, up, and down command shape
- Colima repair and retry hooks
- attached-session signal handling and closeout

## Candidate Backend Direction

Apple Containers 1.2 is a prototype candidate with the future id
`apple-container`. This contract does not add it to the supported registry.

The prototype removed mandatory Compose invocation from `ContainerBackend` and
introduced a typed, backend-neutral effective stack plan plus semantic stack
operation identity. Docker and Colima retain Compose adapters; a native backend
does not need to invent one.

This is only the planning seam. The Apple executor is not registered and does
not yet implement the complete manager operation family. Attach, copy,
streaming logs, runtime repair, gateway activation, secrets, SSH-agent policy,
Rosetta selection, stats reports, and project data operations remain outside
its advertised capabilities.

Required manager-owned semantic operations include:

- materialize, start, stop, remove, and inspect a project stack
- exec, logs, copy, stats, image, volume, and published-port operations
- runtime capability reporting and unsupported-input diagnostics
- readiness, recovery, attached-session, and cleanup reports

Docker and Colima adapters may render the plan to Compose. An Apple adapter may
render it to native `container` operations. Direct Compose files remain outside
the Apple candidate scope.

No automatic detection selects Apple Containers. The completed prototype is
watch-only because the compatibility gates in
`006-compose-backend-compatibility.md` do not all pass.

## Runner Rules

Runner code must call `ContainerManager` or an approved
`effigy-containers`/`effigy-runtime` adapter for container operations.

Runner code must not:

- call `resolve_compose_backend()` after migration
- construct local `docker`, `colima`, `nerdctl`, or Apple `container` commands
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
  `scripts/check-runtime-container-drift.rhai` and the active closeout lane

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

Future Apple promotion additionally requires:

- stack-plan operations do not require a Compose invocation from native
  backends
- explicit candidate selection cannot affect Docker/Colima detection
- unsupported Compose-only inputs fail before side effects
- native lifecycle, readiness, recovery, and cleanup reports match the manager
  contract

Lightweight drift check:

```sh
effigy qa:architecture:runtime-container-drift
```

The command should pass with only documented, path-scoped allowances.
