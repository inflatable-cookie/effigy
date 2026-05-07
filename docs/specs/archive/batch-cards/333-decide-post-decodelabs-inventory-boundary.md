# 333 Decide Post-Inventory Decodelabs Strategy Boundary

Status: complete
Updated: 2026-05-01
Roadmap: `g03.003`
Spec: `docs/specs/026-decodelabs-production-strategy-scope-strict-lane.md`

## Objective

Turn the first Decodelabs production inventory into one explicit next-boundary
decision instead of letting the lane drift.

## In Scope

- decide whether any part of the current Decodelabs production story belongs in
  the neutral deployment model now
- decide whether the future Decodelabs deployment target is:
  - dedicated-host export
  - managed-provider export
  - or an explicit operator-owned stop for now
- tighten the roadmap and contract surfaces so later work is sequencing, not
  rediscovery

## Out Of Scope

- implementing a Decodelabs exporter
- inventing provider templates
- broad application or bundle cleanup

## Acceptance Criteria

- the next Decodelabs deployment boundary is explicit
- the strategy lane no longer depends on vague future intent
- the next batch after `333` can be named directly

## Validation

- `./target/debug/effigy docs check-paths docs/contracts/010-decodelabs-production-strategy.md docs/specs/026-decodelabs-production-strategy-scope-strict-lane.md docs/specs/batch-cards/332-inventory-decodelabs-production-deployment-shape.md docs/specs/batch-cards/333-decide-post-decodelabs-inventory-boundary.md docs/specs/README.md docs/specs/batch-cards/README.md docs/roadmaps/README.md docs/roadmaps/g03/README.md docs/roadmaps/g03/003-decodelabs-production-strategy-scope.md docs/contracts/README.md`

## Decision

Close the lane with Decodelabs explicitly operator-owned for now.

No provider-adapter or dedicated-host export lane opens from this batch.

## Next Task

No active next task inside this card. The boundary decision is complete.
