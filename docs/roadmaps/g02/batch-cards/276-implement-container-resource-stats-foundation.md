# 276 Implement Container Resource Stats Foundation

Status: archived
Updated: 2026-04-18
Roadmap: `g02.016`
Spec: `docs/specs/016-multi-project-coordination-strict-lane.md`

## Objective

Make the next user-visible `g02.016` coordination surface real by adding one
 honest cross-project resource view for running Effigy-managed containers.

## Context

`272` landed the shared status/dashboard surface. `274` landed generated-compose
port auto-allocation on the bounded product-owned path. The next coordination
gap is still operational visibility: operators can see what is running and
where it routes, but not what it is consuming.

The substrate already exists:

- running environment discovery from `effigy container status --all`
- compose project and repo correlation in the container surface
- Colima/compose-backed execution ownership inside the existing container
  runner path

## In Scope

- add `effigy container stats --all` to the CLI/help/parser/runner surface
- collect live CPU and memory stats for running Effigy-managed containers
  across repos
- group/render those stats by repo and compose project in text and JSON
- keep output honest when stats are unavailable, partial, or containers exit
  during collection
- add focused coverage in the affected CLI, runner, and container crates
- update docs/help/changelog for the new bounded behavior

## Out Of Scope

- repo-local `container stats` submodes beyond what the batch strictly needs
- historical metrics, polling loops, dashboards, or charts
- gateway metrics
- shared-service orchestration
- `g02.015` persistent data or volume surfaces

## Acceptance

- `effigy container stats --all` works without a repo override and reports
  live resource usage for running Effigy-managed containers across repos
- text and JSON output correlate repo, compose project, container/service, and
  CPU/memory usage honestly
- the surface degrades clearly when runtime stats are unavailable
- focused tests cover parser/help/runner and container-side shaping

## Result

This batch is now landed.

What changed:

- `effigy container stats --all` is now a real CLI/help/parser/runner surface
- cross-project running-environment discovery is now reused for one bounded
  resource view instead of inventing a parallel path
- the container surface now collects live CPU and memory stats from the
  runtime for discovered Effigy-managed containers and renders them in text
  and JSON
- the stats surface stays honest when runtime stats are partial or unavailable
  by emitting a warning and leaving affected service samples empty instead of
  failing the whole report
- focused tests cover parser/help, runner `--repo` rejection, and
  container-side stats parsing/report shaping

## Next Task

Stop in planning and decide whether `g02.016` still wants shared services or
an explicit bounded deferral.
