# 315 Plan First Render Export Contract

Status: archived
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Define the first provider-export contract for `deploy.model.v1` by planning the
Render adapter surface.

## In Scope

- decide the first `effigy deploy export render` command boundary
- define the generated file set for the first Render export
- define how `deploy.model.v1` services, domains, backing services, and secrets
  map into Render concepts
- define the warning and block conditions that should prevent export
- keep the lane Underlay-first and provider-template-only

## Out Of Scope

- Railway export
- Decodelabs export
- live provisioning
- provider API integration
- final implementation of the Render exporter

## Acceptance Criteria

- the first Render adapter boundary is explicit
- the generated file set is concrete enough to implement without reopening the
  neutral model
- export-blocking versus warning-only cases are defined clearly
- the next follow-up is explicit, even if it is one more neutral-model
  strengthening slice instead of adapter code

## Validation

- `./target/debug/effigy docs check-paths docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/g03/batch-cards/314-decide-post-production-metadata-widening.md docs/roadmaps/g03/batch-cards/315-plan-first-render-export-contract.md docs/specs/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

After `315`, execute:

- [`316-strengthen-static-fallback-ownership-for-render-export.md`](./316-strengthen-static-fallback-ownership-for-render-export.md)
