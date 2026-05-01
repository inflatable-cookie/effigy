# 312 Decide Post Deploy-Model Foundation Widening

Status: landed
Updated: 2026-04-30
Roadmap: `g03.001`
Spec: `docs/specs/001-production-deployment-model-and-export-contract-strict-lane.md`

## Objective

Choose the next bounded `g03.001` move now that `311` landed the first
Underlay-only `deploy model --json` foundation.

## Scope

- reassess the deployment-export lane against the now-live neutral model
- decide whether the next widening seam is:
  - direct provider export work
  - or one more neutral-model strengthening batch first
- refresh the strict-lane front doors so `continue` resolves through the real
  next card instead of a planning void

## Out Of Scope

- implementing provider exporters
- widening Decodelabs production automation
- reopening the neutral-model foundation itself

## Acceptance

- one explicit next execution card exists for `g03.001`
- the strict-lane front doors stop advertising `311` as still active
- the chosen next batch matches the known gaps in the current contracts and
  example model

## Decision

The next bounded `g03.001` move should be one more neutral-model strengthening
batch before any provider adapter work starts.

Why this comes first:

- the current example and derivation contracts still leave three important
  provider-facing questions open:
  - static-service output-path ownership
  - release-hook promotion
  - health-probe promotion
- pushing straight into Render or Railway export would either:
  - bury those decisions inside adapter-specific logic
  - or force thin adapters to emit placeholders without a trustworthy neutral
    model contract behind them
- the architecture lane already said provider exporters should stay thin and
  derive from stable model truth, not invent missing deployment semantics

## Result

The next explicit `g03.001` execution batch is now card `313`.

## Next Task

Execute `313` to strengthen `deploy.model.v1` with the missing production
metadata seams before any provider adapter work begins.
