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

- [`007-distribution-release-and-consumer-rollout-strict-lane.md`](./007-distribution-release-and-consumer-rollout-strict-lane.md)
- [`batch-cards/README.md`](./batch-cards/README.md)

Paused but still useful:

- [`010-effigy-modularization-and-crate-boundaries-strict-lane.md`](./010-effigy-modularization-and-crate-boundaries-strict-lane.md)

Recently completed:

- [`013-dev-front-door-and-managed-lifecycle-strict-lane.md`](./013-dev-front-door-and-managed-lifecycle-strict-lane.md)
- [`015-persistent-data-and-volume-lifecycle-strict-lane.md`](./015-persistent-data-and-volume-lifecycle-strict-lane.md)
- [`016-multi-project-coordination-strict-lane.md`](./016-multi-project-coordination-strict-lane.md)
- [`014-rust-native-gateway-strict-lane.md`](./014-rust-native-gateway-strict-lane.md)
- [`012-container-context-and-transparent-execution-strict-lane.md`](./012-container-context-and-transparent-execution-strict-lane.md)
- [`011-service-catalog-and-compose-assembly-strict-lane.md`](./011-service-catalog-and-compose-assembly-strict-lane.md)

## Next Task

Resume `g02.007` and the `v0.3` release-prep path.
