# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning
- keep the active tree focused on live or near-live cards; archive stale cards
  once their lane is closed or paused cleanly
- do not use this index as a graveyard dump of every historical card

## Current Live Chain

- [`260-decide-post-modularization-integration-spine-for-g02-011-through-g02-016.md`](./260-decide-post-modularization-integration-spine-for-g02-011-through-g02-016.md)
  is complete. The post-`g02.010` integration order is now explicit:
  `011` first, then `012`, then `014`, then `015`, then `016`, with
  `013` treated as the downstream aggregator milestone.
- [`261-implement-service-catalog-integration-foundation.md`](./261-implement-service-catalog-integration-foundation.md)
  is complete. The first real product integration batch is now landed:
  manifest service declarations, generated compose ownership in
  `effigy-containers`, and schema acceptance all now exist on the product
  path.
- [`262-implement-catalog-and-eject-product-surface.md`](./262-implement-catalog-and-eject-product-surface.md)
  is complete. The visible `g02.011` operator surface is now real:
  `catalog list`, `catalog extract`, and `container eject`.
- [`263-prove-service-catalog-loop-in-one-real-project.md`](./263-prove-service-catalog-loop-in-one-real-project.md)
  is complete. The full generated-compose loop is now proven in
  `underlay-reference`, including an in-batch fix for manifest rewrite during
  `container eject`.
- [`264-implement-context-routing-foundation.md`](./264-implement-context-routing-foundation.md)
  is complete. The first bounded `g02.012` integration slice is now landed:
  manifest context support plus routing integration through normal task
  dispatch.
- [`265-implement-explicit-exec-and-alias-surface.md`](./265-implement-explicit-exec-and-alias-surface.md)
  is complete. The visible `g02.012` exec surface is now landed: explicit
  exec, aliases, CWD mapping, handoff behavior, and a real consumer proof.
- [`266-implement-gateway-command-foundation.md`](./266-implement-gateway-command-foundation.md)
  is complete. The host-native `gateway up/down/status` surface, detached
  daemon path, and startup diagnostics are now landed.
- [`267-implement-gateway-route-registration-foundation.md`](./267-implement-gateway-route-registration-foundation.md)
  is complete. Manifest DNS declaration and container lifecycle route
  registration are now wired into the product path.
- [`268-prove-plain-http-gateway-hostname-loop-in-one-real-project.md`](./268-prove-plain-http-gateway-hostname-loop-in-one-real-project.md)
  is complete. The plain HTTP hostname loop is now proven on one real project,
  and the proof hardened gateway registration with
  `[containers.<name>.dns].port` for multi-port stacks.
- [`269-plan-gateway-tls-closeout-batch.md`](./269-plan-gateway-tls-closeout-batch.md)
  is complete. The remaining gateway TLS work is now bounded on a trustworthy
  product boundary instead of left implicit.
- [`270-implement-gateway-tls-closeout.md`](./270-implement-gateway-tls-closeout.md)
  is complete. The gateway now has a real TLS product path, including
  `setup-tls`, route-owned cert lifecycle, honest readiness/status projection,
  and one real HTTPS consumer proof.

## Archive Rule

- closed or paused lane cards should move to `../archive/batch-cards/` once the
  lane no longer needs them in the active tree
- the active tree should stay focused on the live strict lanes rather than the
  full historical corpus
- use the governing spec plus roadmap to resolve the current ready card; this
  README is only the front door

## Next Task

No further `g02.014` execution lives here. The bounded gateway lane is
complete; broader route-dashboard coordination belongs to `g02.016`.
