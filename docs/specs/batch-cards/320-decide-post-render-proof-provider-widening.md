# 320 Decide Post-Render-Proof Provider Widening

Status: landed
Updated: 2026-05-01
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Decide the next honest provider-widening seam after the first real Render proof.

## In Scope

- assess whether Render needs another widening batch before any second provider
- assess whether Railway planning can now open cleanly
- record the next ready card explicitly

## Out Of Scope

- implementing Railway export in the same card
- Decodelabs production export work

## Decision

Open Railway planning next.

## Why

- the bounded Render adapter now has:
  - contract coverage
  - product-path tests
  - one real Underlay proof
- the proof did not expose model or exporter drift that justifies another
  Render-only batch first
- widening providers now gives the neutral deployment model a more meaningful
  second pressure test than repeating more Render churn

## Result

The next ready card is:

- [`321-plan-first-railway-export-contract.md`](./321-plan-first-railway-export-contract.md)

## Validation

- `./target/debug/effigy docs check-paths docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md docs/specs/README.md docs/specs/batch-cards/README.md docs/specs/batch-cards/319-prove-render-export-in-one-real-underlay-repo.md docs/specs/batch-cards/320-decide-post-render-proof-provider-widening.md docs/roadmaps/README.md docs/roadmaps/g03/README.md`

## Next Task

Execute `321`.
