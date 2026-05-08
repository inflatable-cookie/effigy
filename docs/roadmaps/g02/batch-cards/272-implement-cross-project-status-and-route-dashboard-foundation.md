# 272 Implement Cross-Project Status And Route Dashboard Foundation

Status: archived
Updated: 2026-04-18
Roadmap: `g02.016`
Spec: `docs/specs/016-multi-project-coordination-strict-lane.md`

## Objective

Make the first user-visible `g02.016` coordination surface real by adding
cross-project container status and widening the shared gateway route
dashboard.

## Context

`g02.014` is complete on its bounded product boundary. The next coordination
value is not more gateway plumbing; it is giving operators one honest view of
what Effigy is already running across repos.

The substrate already exists:

- shared route state in `~/.effigy/gateway/routes.json`
- shared port-allocation state in `~/.effigy/ports.json`
- container reports that already carry compose project names, declared ports,
  and DNS metadata

## In Scope

- add `effigy container status --all` to the CLI/help/parser/runner surface
- discover running Effigy-managed container environments across repos and
  render them in text and JSON
- widen `effigy gateway status` so it works as a fuller route dashboard over
  the shared route table, including route ownership/project visibility and
  honest TLS readiness summary
- correlate shared port-allocation state where it materially improves status
  output
- add focused coverage in the affected CLI, runner, container, and gateway
  crates

## Out Of Scope

- automatic port allocation when `host.ports` are omitted
- CPU or memory stats
- shared-service orchestration
- `g02.015` data lifecycle or volume surfaces
- browser/TUI work

## Acceptance

- `effigy container status --all` works without a repo override and reports
  running Effigy-managed environments across repos on one machine
- `effigy gateway status` exposes a fuller shared route dashboard while
  staying honest when the gateway daemon is stopped
- both surfaces have explicit JSON contracts and focused tests
- help/docs reflect the new status surface

## Result

This batch is now landed.

What changed:

- `effigy container status --all` is now a real CLI/help/parser/runner surface
- cross-project status discovery now groups running Effigy-managed compose
  environments by repo and compose project
- the shared status surface now reports declared DNS metadata and any known
  shared port-allocation range
- `effigy gateway status` now renders a fuller route dashboard with route
  owner/project visibility and per-route TLS certificate readiness
- the new parser/help/status contracts are covered in focused tests

## Next Task

Stop in planning and decide the next bounded `g02.016` follow-up: port
auto-allocation, resource stats, or explicit deferral.
