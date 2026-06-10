# 989 - Add Framework Route Entrypoint Edges

Roadmap: [`../040-framework-route-and-entrypoint-edges.md`](../040-framework-route-and-entrypoint-edges.md)
Strict lane: [`../../../specs/091-codegraph-parity-strict-lane.md`](../../../specs/091-codegraph-parity-strict-lane.md)

Status: Complete
Owner: Platform
Created: 2026-05-18

## Purpose

Represent routes, command entrypoints, and task selectors as graph facts agents
can traverse.

## Work

- define route/entrypoint node and edge shapes
- implement Effigy task/command entrypoint facts first
- add one web framework family with real fixture proof
- link route facts to handler symbols when resolvable
- expose route facts through search/node/impact/explore
- document confidence and unsupported framework shapes

## Acceptance

- route-to-handler and handler-to-route queries work for supported fixtures
- unresolved targets are visible and confidence-labeled
- `explore` can use route facts as traversal seeds
- no unsupported framework is implied by docs

## Evidence

- [`2026-05/18-155956-route-entrypoint-edges.md`](../../../logs/archive/2026-05/18-155956-route-entrypoint-edges.md)

## Next Task

Execute `990`.
