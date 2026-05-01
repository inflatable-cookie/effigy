# 323 Decide Post-Railway-Export Foundation Boundary

Status: ready
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Decide the next honest widening seam after the first Railway export
foundation.

## In Scope

- assess whether Railway needs another bounded proof or widening batch first
- assess whether the lane is ready to pause provider work and move back into
  milestone-level sequencing
- record the next ready card explicitly

## Out Of Scope

- implementing the next provider batch in the same card
- Decodelabs production export work

## Acceptance Criteria

- the next widening seam is explicit
- any remaining Railway-specific gaps are named before the lane moves on
- the next ready card is recorded directly

## Validation

- `./target/debug/effigy docs check-paths docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/batch-cards/README.md docs/specs/batch-cards/322-implement-railway-export-foundation.md docs/specs/batch-cards/323-decide-post-railway-export-foundation-boundary.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

Execute `323`.
