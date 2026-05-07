# 322 Implement Railway Export Foundation

Status: landed
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

## Result

The first Railway export foundation is now landed.

Shipped surface:

- `effigy deploy export railway --path <DIR> [--plan] [--json]`
- service-local `railway.toml` generation for Underlay `front`, `admin`,
  `api`, and optional `jobs`
- machine-facing `report.json` with explicit follow-up for:
  - public domains
  - `DATABASE_URL`
  - provider-side Postgres creation

The batch also proved the shape against one real Underlay repo:

- `underlay-reference`

That proof showed the bounded Railway artifact bundle was coherent enough to
close the implementation batch without another immediate product correction
slice.

## Validation

- targeted Railway export tests
- targeted deploy-model tests when the adapter needs widened expectations
- `./target/debug/effigy docs check-paths docs/contracts/008-railway-export-contract.md docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/batch-cards/README.md docs/specs/batch-cards/322-implement-railway-export-foundation.md`
- `./target/debug/effigy deploy export railway --repo /Users/tom/Dev/projects/underlay-reference --path /tmp/effigy-railway-proof-underlay-reference --plan`
- `./target/debug/effigy deploy export railway --repo /Users/tom/Dev/projects/underlay-reference --path /tmp/effigy-railway-proof-underlay-reference`

## Next Task

After `322`, execute:

- [`323-decide-post-railway-export-foundation-boundary.md`](./323-decide-post-railway-export-foundation-boundary.md)
