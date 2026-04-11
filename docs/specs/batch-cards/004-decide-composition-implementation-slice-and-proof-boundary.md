# 004 Decide Composition Implementation Slice And Proof Boundary

Status: ready
Updated: 2026-04-11
Roadmap: `g02.002`
Spec: `docs/specs/002-manifest-composition-and-override-strict-lane.md`

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

Complete this planning batch, then either open the first implementation-ready
card or return the lane to an explicit intent checkpoint.
