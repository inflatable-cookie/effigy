# 260 Decide Post-Modularization Integration Spine For g02.011 Through g02.016

Status: landed
Updated: 2026-04-17
Roadmap: `g02.011`, `g02.012`, `g02.013`, `g02.014`, `g02.015`, `g02.016`
Spec: `docs/specs/011-service-catalog-and-compose-assembly-strict-lane.md`, `docs/specs/012-container-context-and-transparent-execution-strict-lane.md`

## Objective

Turn the crate-first prep work for the remaining container and execution lanes
into one explicit integration order and one bounded first implementation batch.

## Context

`g02.010` is complete. The remaining `g02` product work is no longer blocked
by architecture cleanup, and `g02.007` is intentionally deferred until the
rest of the `g02` spine ships for one `v0.3` release cut.

Several crate-first lanes already have real shipped libraries:

- `effigy-catalog` (`g02.011`)
- `effigy-exec` (`g02.012`)
- `effigy-gateway` (`g02.014`)
- partial volume/ports foundations that feed `g02.015` and `g02.016`

What is still missing is runner integration order, ownership boundaries in
`src/`, and the first bounded write set that starts landing those product
surfaces without collapsing back into root-crate churn.

## In Scope

- decide the integration order across `g02.011`–`g02.016`
- decide which lane owns the first implementation batch
- define the first bounded write set and acceptance target
- refresh the roadmap/spec front doors so the active next move is explicit

## Out Of Scope

- release execution
- cross-repo rollout work in `g02.008`, `g02.009`, or later research-intake
  follow-up
- broad speculative redesign beyond the shipped crate foundations

## Acceptance Criteria

- one explicit post-`g02.010` integration order is recorded
- one bounded next implementation batch is ready
- the front-door planning surfaces no longer imply that `g02.007` is next

## Decision

The post-`g02.010` integration order is:

1. `g02.011` service catalog integration foundation
2. `g02.012` transparent execution integration
3. `g02.014` gateway integration
4. `g02.015` persistent data lifecycle integration
5. `g02.016` multi-project coordination integration
6. `g02.013` managed `effigy dev` as the downstream aggregator milestone

Why this order:

- `011` is the root product wiring for the new container system; without it,
  `012` and `015` would still be integrating against a half-real surface.
- `012` is the next highest leverage because transparent execution depends on
  the container/runtime boundary being real and then feeds directly into the
  eventual `effigy dev` front door.
- `014` can partly integrate later in parallel, but route registration becomes
  much more valuable once the container spine is no longer crate-only.
- `015` and `016` are downstream value layers, not the first structural
  product-wiring move.
- `013` is explicitly the aggregator milestone and should land after its
  prerequisites exist in the product, not as a speculative front door first.

## Next Task

Card `261` — implement the first `g02.011` service-catalog integration
foundation in the root product surface.
