# 003 Decide Override, Conflict, And Explainability

Status: ready
Updated: 2026-04-11
Roadmap: `g02.002`
Spec: `docs/specs/002-manifest-composition-and-override-strict-lane.md`

## Objective

Turn the root composition direction into an operator-usable contract by deciding:

- what `override` actually permits
- what still fails as a conflict
- how the effective composed manifest should be explained and inspected

## In Scope

- define additive merge defaults vs explicit override behavior
- define the initial override granularity boundary
- define conflict classes that always fail
- define the minimum explainability/operator surface for composed manifests
- update roadmap/spec/currentness surfaces so the next batch is explicit

## Out Of Scope

- parser/runtime implementation
- feature-specific demo config design
- broad refactors of existing manifests into fragments
- final polish of every future inspection command

## Acceptance Criteria

- `g02.002` clearly states how override intent interacts with conflict failure
- the design names which merge cases remain illegal even under override
- the minimum effective-manifest inspection posture is explicit
- the active front-door surfaces point at the true next step

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the override model proves broader than one bounded planning batch
- human intent is needed to choose between materially different merge postures

## Next Task

Complete this planning batch, then either open the next ready card for
implementation-shaping work or return the lane to an explicit intent checkpoint.
