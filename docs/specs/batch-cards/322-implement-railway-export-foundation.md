# 322 Implement Railway Export Foundation

Status: ready
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Implement the first bounded Railway export surface on top of
`deploy.model.v1`.

## In Scope

- add the first `effigy deploy export railway` command path
- generate service-local `railway.toml` files for the shipped Underlay shape
- generate the first machine-facing `report.json`
- map operator follow-up honestly for:
  - domains
  - `DATABASE_URL`
  - provider-side Postgres creation
- emit warnings or block when the model still exceeds the bounded Railway
  contract

## Out Of Scope

- Railway API integration
- live provisioning
- Decodelabs export
- project-level secret or domain automation

## Acceptance Criteria

- `effigy deploy export railway` exists as a bounded file-generation surface
- Underlay export produces a coherent first Railway artifact bundle
- operator follow-up stays explicit in `report.json`
- unsupported model seams fail honestly instead of being guessed

## Validation

- targeted Railway export tests
- targeted deploy-model tests when the adapter needs widened expectations
- `./target/debug/effigy docs check-paths docs/contracts/008-railway-export-contract.md docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/batch-cards/README.md docs/specs/batch-cards/322-implement-railway-export-foundation.md`

## Next Task

Execute `322`.
