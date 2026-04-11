# 002 Decide Composition Contract Shape

Status: ready
Updated: 2026-04-11
Roadmap: `g02.002`
Spec: `docs/specs/002-manifest-composition-and-override-strict-lane.md`

## Objective

Make the first bounded design decision for manifest composition:

- what the root composition surface should be
- what fragment shape Effigy composes
- how path resolution works
- where explicit override intent belongs

## In Scope

- compare include/require/import framing only as needed to settle the contract
- decide whether fragments are partial manifests or full standalone manifests
- decide path resolution rules
- define the initial override and conflict boundary clearly enough for later
  design work
- update roadmap/spec/currentness surfaces so the next batch is explicit

## Out Of Scope

- parser/runtime implementation
- feature-specific config loading for demos or any other surface
- desktop/TUI work
- broad cleanup of existing manifests into split files

## Acceptance Criteria

- `g02.002` clearly states the preferred root composition model
- override behavior is named as a first-class contract concern, not a future
  footnote
- the decision is explicit enough that `g02.003` can plan demos without
  inventing demo-only loading semantics
- the active front-door surfaces point at the true next step

## Validation

- `git diff --check`
- `effigy qa:docs`

## Stop Conditions

- the design space proves too broad for one bounded planning batch
- human intent is needed to choose between materially different product
  postures

## Next Task

Complete this planning batch, then either open the next ready card for override
details/explainability or return the lane to an explicit intent checkpoint.
