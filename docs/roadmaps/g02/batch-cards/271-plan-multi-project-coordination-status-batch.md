# 271 Plan Multi-Project Coordination Status Batch

Status: archived
Updated: 2026-04-18
Roadmap: `g02.016`
Spec: `docs/specs/016-multi-project-coordination-strict-lane.md`

## Objective

Turn broad `g02.016` coordination goals into one bounded first product batch
now that the gateway lane is closed.

## Scope

- assess the coordination substrate that is already shipped
- decide the smallest trustworthy first product batch for `g02.016`
- record what should stay out of that first batch
- refresh the front-door planning surfaces so `continue` resolves to one
  explicit execution card

## Out Of Scope

- implementing the coordination batch itself
- `g02.015` persistent data lifecycle work
- shared-service orchestration
- CPU or memory stats
- automatic host-port allocation for omitted `host.ports`

## Acceptance

- one explicit first execution card exists for `g02.016`
- the first batch is bounded on a real product surface rather than roadmap
  ambition
- the front-door planning surfaces stop pointing at `g02.016` only in the
  abstract

## Decision

The first `g02.016` batch should start with visibility, not more mutation.

What is already real:

- `effigy-gateway::ports::PortRegistry` already persists shared project port
  allocations at `~/.effigy/ports.json`
- gateway route registration already maintains one shared route table under
  `~/.effigy/gateway/routes.json`
- `effigy gateway status` already reads that shared state, but only as a
  narrow gateway-lifecycle surface
- container policy/report shaping already knows compose project names,
  declared host ports, and DNS metadata

What is missing in the product:

- no `effigy container status --all` for cross-project visibility
- no fuller route-dashboard shape in `effigy gateway status`
- no honest shared operator view that ties together running environments and
  registered domains

The smallest trustworthy first batch is therefore read-only coordination:

- add `effigy container status --all` to discover and report running
  Effigy-managed environments across repos
- widen `effigy gateway status` into a fuller route dashboard over the shared
  route table, including route owner/project visibility and honest TLS status
- reuse the shipped port-registry substrate only where it makes status more
  informative; do not make auto-allocation a dependency of the batch

What stays out:

- assigning ports automatically when manifests omit `host.ports`
- CPU and memory usage collection
- shared service lifecycles
- broader multi-project mutation or orchestration

## Result

The first explicit `g02.016` execution batch is now card `272`.

## Next Task

Execute `272` to land the first coordination surface.
