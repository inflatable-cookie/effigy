# 321 Plan First Railway Export Contract

Status: landed
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Define the first bounded Railway export contract on top of `deploy.model.v1`.

## In Scope

- define the first Railway-owned generated file surface
- map Underlay-derived `static`, `web`, `worker`, and managed `postgres`
  entries into a bounded Railway contract
- decide what Railway should satisfy natively versus what Effigy must warn
  about or leave manual
- record any remaining neutral-model gaps before adapter implementation opens

## Out Of Scope

- implementing `deploy export railway`
- live provisioning
- Decodelabs production export work

## Acceptance Criteria

- the first Railway export boundary is documented in contracts or planning
- the next implementation seam is explicit
- any remaining neutral-model gaps are recorded before adapter work begins

## Result

The first Railway contract is now explicit in:

- [`docs/contracts/008-railway-export-contract.md`](../../contracts/008-railway-export-contract.md)

It locks the first honest Railway shape to:

- service-local `railway.toml` files
- one machine-facing `report.json`
- operator-owned domains and variable wiring
- provider-side Postgres creation as explicit follow-up, not fake automation

No new neutral-model gap was exposed that should block implementation. The next
honest seam is now a bounded Railway export foundation batch.

## Validation

- `./target/debug/effigy docs check-paths docs/contracts/README.md docs/contracts/008-railway-export-contract.md docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/batch-cards/README.md docs/specs/batch-cards/321-plan-first-railway-export-contract.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

After `321`, execute:

- [`322-implement-railway-export-foundation.md`](./322-implement-railway-export-foundation.md)
