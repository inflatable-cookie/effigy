# 264 Implement Context Routing Foundation

Status: landed
Updated: 2026-04-18
Roadmap: `g02.012`
Spec: `docs/specs/012-container-context-and-transparent-execution-strict-lane.md`

## Objective

Land the first bounded `g02.012` integration slice by wiring transparent
container routing into normal task dispatch without widening into aliases,
handoff, or broad CLI churn.

## Scope

- add container `context` support to the manifest surface
- add the minimum task metadata needed for routing decisions
- wire `effigy-exec::routing::route()` into normal task dispatch
- keep host-native commands on the host path
- fail clearly when a routed container context is not running
- preserve the current clean shell/domain boundary during integration

## Acceptance

- manifests can declare a container execution context for normal task routing
- routed tasks use `effigy-exec` decisions instead of ad hoc runner logic
- host-native commands still bypass container routing
- a missing or stopped target container produces a clear product error
- the write set stays bounded to manifest + runner integration glue

## Outcome

The first bounded `g02.012` integration slice is now landed.

- manifests can declare `context = "dev"` on a container
- manifest tasks can declare `run_in = "host"`
- standard task dispatch now calls `effigy-exec::routing::route()`
- routed standard tasks execute through non-interactive container exec instead
  of ad hoc host-only logic
- routed tasks fail clearly when the target container is not running
- host-native command names were updated to treat `service`, `catalog`, and
  `catalogue` as host-only operator surfaces

This batch deliberately stopped before explicit `effigy exec`, aliases, CWD
mapping, and effigy-in-container handoff.

## Next Task

Execute `265`.
