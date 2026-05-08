# 318 Decide Post-Render-Export Foundation Boundary

Status: archived
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Decide the next honest widening seam after the first Render export foundation.

## In Scope

- assess whether Render needs one more bounded proof or widening batch first
- assess whether the lane is ready to open Railway planning next
- record the next ready card explicitly

## Out Of Scope

- implementing the next provider batch in the same card
- Decodelabs production export work

## Decision

Do one real Underlay proof batch for Render before opening Railway planning.

## Why

- the Render adapter now exists and the bounded product tests are green
- but `g03.002` still promises one real consumer proof, not just fixture-level
  coverage
- opening Railway now would widen providers before the first provider had a
  real-repo proof boundary

## Result

The next ready card is:

- [`319-prove-render-export-in-one-real-underlay-repo.md`](./319-prove-render-export-in-one-real-underlay-repo.md)

## Validation

- `./target/debug/effigy docs check-paths docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/README.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/g03/batch-cards/317-implement-render-export-foundation.md docs/roadmaps/g03/batch-cards/318-decide-post-render-export-foundation-boundary.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

Execute `319`.
