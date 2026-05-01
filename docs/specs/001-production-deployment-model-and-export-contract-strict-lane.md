# 001 Production Deployment Model And Export Contract Strict Lane

Status: active
Updated: 2026-04-30
Roadmap: `g03.001`

## Context

Effigy now has the planning foundation for production deployment export:

- architecture direction
- neutral deployment-model contract
- first Underlay derivation contract
- first concrete `underlay-reference` example

What it does not have yet is a runtime surface.

This lane owns the first bounded implementation path for:

- `effigy deploy model --json`
- neutral model derivation
- one real Underlay proof boundary

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/architecture/021-production-deployment-export-architecture.md`
- `docs/contracts/002-production-deployment-model.md`
- `docs/contracts/003-underlay-deployment-derivation.md`
- `docs/contracts/004-underlay-reference-deploy-model-example.md`
- `docs/roadmaps/g03/README.md`
- `docs/roadmaps/g03/001-production-deployment-model-and-export-contract.md`

## Lane Focus

This lane owns:

- the first `deploy` command surface
- `deploy model --json`
- neutral deployment-model derivation for Underlay only
- a trustworthy `deploy.model.v1` JSON payload
- warning semantics strong enough for later provider export

This lane does not yet own:

- provider file export
- live provisioning
- Decodelabs production automation

## Current Posture

`strict-ready`

The correct implementation order is:

1. land `deploy model --json` with one bounded Underlay-only derivation path
2. prove the emitted payload against the `underlay-reference` example
3. decide the next widening seam before opening Render or Railway export work

## Integration Constraint

- keep the first batch Underlay-only
- keep the first batch JSON-only; do not mix in text rendering or provider
  output yet
- derive only from effective manifest and bundle state
- prefer warnings over fake production defaults
- do not let provider-template work start before the neutral payload is stable

## Continuation Chain

1. `311` — implement `deploy model --json` foundation for Underlay
2. `312` — decide post-model-foundation widening
3. `313` — strengthen deploy-model production metadata foundation
4. later — open provider export batches

## Exit Condition

This strict lane is complete when:

- `effigy deploy model --json` exists
- Underlay repos derive into `deploy.model.v1`
- `underlay-reference` proves the shape
- the warning boundary is trustworthy enough that provider export can stay thin

## Next Task

Execute `313` — strengthen `deploy.model.v1` with the missing production
metadata seams before provider adapter work begins.
