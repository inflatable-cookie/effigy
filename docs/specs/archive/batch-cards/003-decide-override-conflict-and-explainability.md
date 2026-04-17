# 003 Decide Override, Conflict, And Explainability

Status: complete
Updated: 2026-04-11
Roadmap: `g02.002`
Spec: `docs/specs/archive/002-manifest-composition-and-override-strict-lane.md`

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

## Decision

The contract now treats override as fine-grained include-site intent, not a
whole-fragment hammer.

Settled direction:

- override remains declared on include entries
- override should target explicit config paths such as `tasks.dev` or
  `release.sync_files`
- additive merge is the default for distinct table keys
- conflicting scalar/list values fail unless the full path is explicitly listed
  in `override`
- the first contract only supports whole-value replacement at the addressed
  path, not patch expressions or array-element surgery
- explainability must show include order, effective sources, and conflict
  origins in both text and JSON

This addresses the practical concern that a boolean `override = true` is too
coarse when a repo only wants to replace one value in a larger composed
fragment.

## Next Task

Open the next ready card for implementation-shaping scope and feature
compatibility proof, now that the contract shape is explicit enough to stop
future feature-local loading semantics.
