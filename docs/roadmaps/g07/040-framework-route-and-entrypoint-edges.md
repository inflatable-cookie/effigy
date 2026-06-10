# g07.040 - Framework Route And Entrypoint Edges

Status: Complete
Depends on: `g07.039`

## Goal

Teach the graph about web and task entrypoints so agents can trace from an
external route, command, or task selector to implementation owners.

## Scope

- define a route/entrypoint node and edge model
- support high-value families first:
  - Effigy task selectors and command surfaces
  - PHP/Laravel routes and controller handlers
  - Express/Fastify-style JavaScript/TypeScript routes
  - Python Flask/FastAPI/Django routes after Python extractor support
  - Rust Axum/Actix/Rocket-style patterns where practical
- link route facts to handler symbols when resolution is reliable
- emit unresolved route targets with confidence when resolution is heuristic
- expose route facts through `search`, `node`, `impact`, and `explore`
- add tests for route-to-handler and handler-to-route queries

## Guardrails

- no framework-specific magic outside extractor-owned modules
- no fake exact edges when only text matching occurred
- no route parser that breaks normal file indexing on unsupported syntax
- no route coverage claim without fixture-backed examples

## Acceptance Criteria

- agents can ask "where is /foo handled?" or "what entrypoints call this
  handler?" and get useful graph output
- `explore` can use route edges as traversal seeds
- route JSON payloads preserve confidence and provenance
- docs list supported framework shapes and unsupported cases

## Evidence

- [`2026-05/18-155956-route-entrypoint-edges.md`](../logs/archive/2026-05/18-155956-route-entrypoint-edges.md)

## Next Task

Execute `990` after route facts can feed `explore`.
