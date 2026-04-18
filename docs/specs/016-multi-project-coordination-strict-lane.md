# 016 Multi-Project Coordination Strict Lane

Status: active
Updated: 2026-04-18
Roadmap: `g02.016`

## Context

The bounded gateway lane is complete, but the broader coordination value it
unlocked is not. Effigy now has shared route and port state, yet the product
still makes operators inspect one repo at a time.

This lane owns the next integration layer: cross-project visibility first,
then any broader coordination that still earns its complexity.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/roadmaps/g02/README.md`
- `docs/roadmaps/g02/016-multi-project-coordination.md`
- `docs/architecture/020-container-infrastructure-design.md`

## Lane Focus

This lane owns:

- cross-project container visibility through the product CLI
- shared gateway route/dashboard visibility on top of the shipped route table
- follow-through on the shipped `PortRegistry` substrate where it materially
  improves coordination surfaces
- bounded later expansion into port auto-allocation, resource visibility, or
  shared services only if those still look justified after the status layer is
  real

## Current Posture

`active`

Shipped substrate that this lane builds on:

- `effigy-gateway::ports::PortRegistry` already persists project allocations at
  `~/.effigy/ports.json`
- gateway route registration already writes one shared route table under
  `~/.effigy/gateway/routes.json`
- `effigy gateway status` already reports gateway lifecycle plus registered
  routes from that shared state
- container policy/report shaping already carries compose project names,
  declared ports, and DNS metadata
- the gateway crate already tracks runtime stats internally, but the product
  does not yet use that as a coordination surface

## Integration Constraint

This lane should start with read-only coordination surfaces before widening
into heavier mutation or orchestration:

- make cross-project status honest before adding more automation
- keep `g02.015` data lifecycle work separate instead of blending volume and
  coordination concerns into one lane
- keep `g02.013` as the downstream aggregator rather than pulling `effigy dev`
  concerns forward
- treat shared services and resource stats as later value checks, not assumed
  must-build work

## Remaining Integration Work

The bounded continuation chain now starts with:

1. `271` — plan the first `g02.016` execution batch on a trustworthy product
   boundary instead of jumping into broad coordination work
2. `272` — first execution batch: cross-project status foundation through
   `effigy container status --all` plus a fuller shared route dashboard in
   `effigy gateway status`

What is now real in the product path:

- `effigy container status --all` discovers running Effigy-managed compose
  environments across repos without requiring a repo override
- the cross-project status surface correlates repo, compose project,
  primary service, declared DNS metadata, and any known shared port-allocation
  range
- `effigy gateway status` now acts as a fuller shared route dashboard instead
  of only a narrow lifecycle probe, including route owner/project visibility
  and per-route TLS certificate readiness

Later continuation should stay explicit after `272` lands rather than guessed
from the roadmap.

## Exit Condition

This strict lane is complete when:

- the product has a real cross-project status surface
- the shared gateway route/dashboard view is honest enough to support
  simultaneous multi-project use
- any remaining auto-allocation, resource visibility, or shared-service work
  is either shipped or explicitly deferred on a trustworthy boundary

## Next Task

The first `g02.016` execution batch is now landed. Stop in planning and decide
the next bounded follow-up: port auto-allocation, resource stats, or explicit
deferral.
