# 323 Decide Post-Railway-Export Foundation Boundary

Status: archived
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

## Decision

Close `g03.001` and return to milestone-level sequencing.

## Why

- `deploy.model.v1` exists and is tested
- Render export exists and is proven in one real Underlay repo
- Railway export now exists and is also proven against one real Underlay repo
- the remaining work is no longer a strict-lane execution problem
- widening further without a fresh milestone decision would just keep the lane
  open by inertia

## Result

There is no next ready card.

`g03.001` is complete and the specs front doors should stop advertising an
active deploy-export execution lane until a new milestone explicitly opens one.

## Validation

- `./target/debug/effigy docs check-paths docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/roadmaps/g04/batch-cards/README.md docs/roadmaps/g03/batch-cards/322-implement-railway-export-foundation.md docs/roadmaps/g03/batch-cards/323-decide-post-railway-export-foundation-boundary.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

No active ready card. Stop in planning and decide the next `g03` milestone
deliberately.
