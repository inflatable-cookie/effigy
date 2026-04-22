# Specs

Specs hold provisional planning surfaces for active Effigy work.

They are not a second architecture or a duplicate roadmap. Use them when a
lane needs tighter execution grammar than the roadmap alone provides.

## Working Rule

- use specs for active planning and bounded execution control
- promote durable product or behavior rules into architecture or contracts
- keep `docs/specs/` mostly limited to active or still-useful planning
- archive or remove stale specs once the durable outcome is carried elsewhere
- before roadmap generation rollover, purge stale generation-specific specs and
  batch cards from the active tree so the next generation does not inherit dead
  planning debris

Historical command-reference rule:

- active specs may preserve wrapper-script or old command references when they
  are documenting the planning state that existed at the time
- do not treat those references as current operator guidance unless the same
  command is still present in active guides/contracts
- current release/runtime/operator guidance lives in the active guides and
  contracts, not in old planning text

## Active Spec Set

- [`020-multi-project-gateway-expansion-and-service-dns-strict-lane.md`](./020-multi-project-gateway-expansion-and-service-dns-strict-lane.md)
- [`batch-cards/README.md`](./batch-cards/README.md)

Queued next-lane specs:

- [`007-distribution-release-and-consumer-rollout-strict-lane.md`](./007-distribution-release-and-consumer-rollout-strict-lane.md) — still gated on explicit release intent; resumes ahead of `020` whenever release execution is requested

Paused but still useful:

- [`010-effigy-modularization-and-crate-boundaries-strict-lane.md`](./010-effigy-modularization-and-crate-boundaries-strict-lane.md)

Recently completed:

- [`021-unified-init-and-starter-emission-strict-lane.md`](./021-unified-init-and-starter-emission-strict-lane.md)
- [`013-dev-front-door-and-managed-lifecycle-strict-lane.md`](./013-dev-front-door-and-managed-lifecycle-strict-lane.md)
- [`015-persistent-data-and-volume-lifecycle-strict-lane.md`](./015-persistent-data-and-volume-lifecycle-strict-lane.md)
- [`016-multi-project-coordination-strict-lane.md`](./016-multi-project-coordination-strict-lane.md)
- [`014-rust-native-gateway-strict-lane.md`](./014-rust-native-gateway-strict-lane.md)
- [`012-container-context-and-transparent-execution-strict-lane.md`](./012-container-context-and-transparent-execution-strict-lane.md)
- [`011-service-catalog-and-compose-assembly-strict-lane.md`](./011-service-catalog-and-compose-assembly-strict-lane.md)

## Next Task

`g02.020` is the active strict lane as of 2026-04-22 (see
`docs/logs/2026-04/22-190000-g02-020-re-sequencing-ahead-of-g02-007-and-g02-019.md`).

Execute batch card `303` — loopback-IP allocation and gateway setup
integration. See
[`batch-cards/303-implement-loopback-ip-allocation-and-gateway-setup-foundation.md`](./batch-cards/303-implement-loopback-ip-allocation-and-gateway-setup-foundation.md).

`g02.007` remains queued, still gated on explicit release intent. If release
execution is requested, it resumes ahead of `g02.020` with:

`cargo run --bin effigy -- release prepare --yes --version 0.3.0 --check-gates`
