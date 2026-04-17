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

## Active Spec Set

- [`010-effigy-modularization-and-crate-boundaries-strict-lane.md`](./010-effigy-modularization-and-crate-boundaries-strict-lane.md)
- [`007-distribution-release-and-consumer-rollout-strict-lane.md`](./007-distribution-release-and-consumer-rollout-strict-lane.md)
- [`011-service-catalog-and-compose-assembly-strict-lane.md`](./011-service-catalog-and-compose-assembly-strict-lane.md)
- [`012-container-context-and-transparent-execution-strict-lane.md`](./012-container-context-and-transparent-execution-strict-lane.md)
- [`batch-cards/README.md`](./batch-cards/README.md)

## Next Task

The release-closure batch is complete, but release execution remains deferred
while live `g02.010` work is still active in the parallel thread.

`g02.010` is reopened post-`250` with a four-card follow-up chain
(`252`–`255`) covering runner-root tidy, doctor-runner extraction,
and test-harness prelude flattening. Card `252` is the ready card;
details in [`batch-cards/README.md`](./batch-cards/README.md).

Once that chain closes, return to `115` for explicit human-approved
release execution.
