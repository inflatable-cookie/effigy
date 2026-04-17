# 004 Decide Composition Implementation Slice And Proof Boundary

Status: complete
Updated: 2026-04-11
Roadmap: `g02.002`
Spec: `docs/specs/archive/002-manifest-composition-and-override-strict-lane.md`

## Objective

Turn the finished composition contract into one bounded implementation-planning
slice:

- what the first parser/runtime batch should cover
- what explainability surface must ship with it
- what proof scope is enough before later features rely on it

## In Scope

- choose the narrowest honest first implementation slice
- define the minimum effective-manifest inspection surface that must land with
  composition
- define the first cross-feature proof boundary
- update roadmap/spec/currentness surfaces so the next batch is explicit

## Out Of Scope

- implementing manifest composition
- starting demo-harness design work
- broad migration of existing manifests into fragments
- polishing every eventual inspection or UI surface

## Acceptance Criteria

- `g02.002` clearly states the first implementation slice
- the proof boundary is concrete enough to prevent under-scoped implementation
- the active front-door surfaces point at the true next step

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the implementation boundary proves too broad for one bounded planning batch
- human intent is needed to choose between materially different rollout shapes

## Next Task

## Decision

The first implementation batch should stay narrow and infrastructure-first.

It must include:

- composed-manifest loading from `effigy.toml` through nested included partial
  fragments
- enforcement of the decided conflict and path-scoped override rules
- clear composition failures through normal manifest parse/doctor surfaces
- one minimal `effigy config` inspection surface for include graph, evaluation
  order, effective sources, and overridden paths

It should not include:

- manifest refactor helpers
- broad real-repo migrations
- demo-specific config support
- wider UX polish beyond the minimum inspection contract

The first proof boundary should be one cross-feature split, preferably `tasks`
plus `docs_policy` or `release`, so composition is proven as a feature-agnostic
model before `g02.003` relies on it.

## Next Task

Open the first implementation-ready card for composed-manifest loading,
inspection, and one cross-feature proof slice.
