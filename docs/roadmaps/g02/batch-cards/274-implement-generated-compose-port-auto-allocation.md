# 274 Implement Generated-Compose Port Auto-Allocation

Status: archived
Updated: 2026-04-18
Roadmap: `g02.016`
Spec: `docs/specs/016-multi-project-coordination-strict-lane.md`

## Objective

Make port auto-allocation real on the bounded product surface Effigy
actually owns: generated compose environments with omitted `host.ports`.

## Context

The first `g02.016` batch landed the shared visibility layer. The next real
problem is still unresolved: multiple generated-compose projects can declare
the same default host ports and collide, even though the product already ships
`PortRegistry`.

Effigy can solve that cleanly only where it owns published port bindings:

- catalog-backed/generated compose output
- manifest-driven container policy and gateway registration

Direct `compose_file` ownership is intentionally not part of this batch.

## In Scope

- allocate a stable project port range in `~/.effigy/ports.json` when an
  Effigy-owned generated compose environment omits `host.ports`
- use that allocation to publish generated compose host ports without needing
  explicit manifest host-port declarations
- keep explicit `host.ports` behavior unchanged
- make container policy, status output, and gateway registration use the
  allocated HTTP port when the environment is running on auto-assigned ports
- add focused coverage in the affected manifest/container/catalog/gateway/runner
  surfaces
- update docs/help/changelog for the new bounded behavior

## Out Of Scope

- auto-allocation for direct user-owned `compose_file` containers
- CPU or memory stats
- shared-service orchestration
- allocation garbage collection or explicit release commands
- `g02.015` data lifecycle work

## Acceptance

- a generated-compose container with omitted `host.ports` starts on a stable
  non-conflicting allocated host-port range
- repeated `up/down/up` for the same project keeps the same allocated range
- explicit manifest `host.ports` still win and bypass auto-allocation
- gateway route registration and status surfaces stay honest when the HTTP port
  is allocated rather than explicitly declared
- focused tests cover the allocation path and the explicit-ports bypass path

## Result

This batch is now landed.

What changed:

- generated compose now rewrites published host ports through the shipped
  `PortRegistry` when `host.ports` is omitted, instead of leaving fragment
  defaults to collide across projects
- explicit manifest `host.ports` is now wired through generated compose too,
  so product-owned generated stacks and route registration both see the same
  effective host-port bindings
- container policy now carries whether ports were explicitly declared plus the
  effective published bindings after generated-compose rewriting
- gateway registration now resolves the right proxied host port for
  auto-assigned generated stacks, including `[containers.<name>.dns].port`
  selection by container port when host ports were not explicitly declared
- focused tests cover stable allocation, explicit-port bypass, and gateway
  route resolution against effective generated bindings

## Next Task

Stop in planning and decide whether the next bounded `g02.016` follow-up
should be resource stats, shared services, or explicit deferral.
