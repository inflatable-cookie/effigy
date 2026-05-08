# 317 Implement Render Export Foundation

Status: archived
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Implement the first bounded Render export surface on top of
`deploy.model.v1`.

## In Scope

- add the first `effigy deploy export render` command path
- generate the first `render.yaml`
- map Underlay-derived `static`, `web`, `worker`, and managed `postgres`
  entries into the Render Blueprint contract
- map operator secrets and provider-satisfied `DATABASE_URL` refs honestly
- emit warnings or block when the model still exceeds the bounded Render
  contract

## Out Of Scope

- Railway export
- Decodelabs export
- live provisioning
- Render API integration
- projects/environments/env-group widening in `render.yaml`

## Acceptance Criteria

- `effigy deploy export render` exists as a bounded file-generation surface
- Underlay export produces a coherent first `render.yaml`
- managed Postgres and `DATABASE_URL` wiring use the planned provider mapping
- unsupported model seams fail honestly instead of being guessed

## Validation

- targeted Render export tests
- targeted deploy-model tests when the adapter needs widened expectations
- `./target/debug/effigy docs check-paths docs/contracts/007-render-export-contract.md docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/g03/batch-cards/317-implement-render-export-foundation.md`

## Next Task

After `317`, execute:

- [`318-decide-post-render-export-foundation-boundary.md`](./318-decide-post-render-export-foundation-boundary.md)
