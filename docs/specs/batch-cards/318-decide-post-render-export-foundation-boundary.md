# 318 Decide Post-Render-Export Foundation Boundary

Status: ready
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

## Acceptance Criteria

- the post-`317` boundary is explicit
- the next `g03.001` ready card is set clearly
- the lane does not advertise both Render follow-up and Railway planning at
  once

## Validation

- `./target/debug/effigy docs check-paths docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/README.md docs/specs/batch-cards/README.md docs/specs/batch-cards/317-implement-render-export-foundation.md docs/specs/batch-cards/318-decide-post-render-export-foundation-boundary.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

Execute `318`.
