# 332 Inventory Decodelabs Production Deployment Shape

Status: complete
Updated: 2026-05-01
Roadmap: `g03.003`
Spec: `docs/specs/026-decodelabs-production-strategy-scope-strict-lane.md`

## Objective

Write down the current Decodelabs production operating shape strongly enough
that Effigy can stop being vague about what it does and does not own.

## In Scope

- inventory the current dedicated-server production shape at the level that
  matters for strategy
- identify which responsibilities are:
  - deploy-model-worthy
  - dedicated-host-specific
  - operator-only for now
- capture the first explicit no-fake-automation boundary for Effigy

## Out Of Scope

- shipping a Decodelabs deploy adapter
- inventing provider templates
- broad local-dev cleanup in Decodelabs repos

## Acceptance Criteria

- the current production shape is captured in the canonical planning surfaces
- operator-owned versus future-Effigy-owned concerns are explicit
- the lane can move to a bounded post-inventory decision without rediscovery

## Validation

- `./target/debug/effigy docs check-paths docs/contracts/010-decodelabs-production-strategy.md docs/specs/026-decodelabs-production-strategy-scope-strict-lane.md docs/specs/batch-cards/332-inventory-decodelabs-production-deployment-shape.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/003-decodelabs-production-strategy-scope.md docs/contracts/README.md`

## Next Task

No active next task inside this card. The inventory slice is complete.
