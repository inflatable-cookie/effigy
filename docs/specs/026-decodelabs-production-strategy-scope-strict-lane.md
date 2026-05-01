# 026 Decodelabs Production Strategy Scope Strict Lane

Status: active
Updated: 2026-05-01
Roadmap: `g03.003`

## Context

Underlay now has a shipped production deployment model plus bounded Render and
Railway export foundations.

That removes the old excuse for keeping Decodelabs vague. The next honest
problem is not provider automation. It is defining what Decodelabs production
actually is, what Effigy should explicitly refuse to claim today, and what
future export track would even make sense.

## Governing Refs

- `docs/contracts/001-working-rules.md`
- `docs/contracts/010-decodelabs-production-strategy.md`
- `docs/architecture/021-production-deployment-export-architecture.md`
- `docs/roadmaps/g03/003-decodelabs-production-strategy-scope.md`
- `docs/roadmaps/g03/README.md`

## Lane Focus

This lane owns:

- inventorying the real Decodelabs production operating shape
- separating deploy-model-worthy concerns from operator-only concerns
- defining the no-fake-automation boundary for Effigy
- narrowing the future target far enough to sequence later work honestly

This lane does not own:

- shipping a Decodelabs provider adapter
- pretending the local-dev bundle is already a production topology
- reopening Underlay deployment work

## Current Posture

`strict-ready`

The correct implementation order is:

1. inventory the current Decodelabs production shape
2. split the concerns into neutral-model, dedicated-host, and operator-owned
   buckets
3. decide the first future target boundary from that split
4. stop once the strategy is explicit enough that later work is sequencing,
   not rediscovery

## Integration Constraint

- prefer truth over completeness
- do not invent provider-ready abstractions just because Underlay now has them
- keep this lane architecture- and contract-heavy rather than code-heavy
- if a future export shape is still materially ambiguous, stop at the strategy
  boundary instead of faking execution readiness

## Continuation Chain

1. `332` — inventory the current Decodelabs production deployment shape
2. `333` — decide the post-inventory Decodelabs strategy boundary

## Exit Condition

This strict lane is complete when:

- the current Decodelabs production story is explicit
- Effigy has a documented no-fake-automation boundary for Decodelabs
- the next real deployment widening step is narrow enough to plan directly

## Next Task

Execute `333` to decide the post-inventory Decodelabs strategy boundary.
