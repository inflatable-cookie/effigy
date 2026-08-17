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

Active strict lane:

- none

Queued strict lanes:

- none

Archived strict lanes:

- completed or paused historical strict lanes live in
  [`archive/`](./archive/)
- `076` through `096` are archived
- [`097`](./archive/097-graph-aware-scan-intelligence-strict-lane.md) archived
  after graph-aware scan intelligence closeout
- [`099`](./archive/099-local-dependency-management-strict-lane.md) archived
  after local dependency management suite closeout
- [`100`](./archive/100-papercuts-discovery-and-capture-strict-lane.md) archived
  after papercuts discovery closeout
- [`101`](./archive/101-explicit-catalog-membership-strict-lane.md) archived
  after explicit catalog membership
- [`102`](./archive/102-unified-test-orchestration-v011.md) archived after
  unified v0.11 test orchestration
- [`103`](./archive/103-pre-release-ci-proof.md) archived after exact-candidate
  hosted CI proof
- [`104`](./archive/104-bun-committed-dependency-pinning.md) archived after
  committed Bun pinning
- [`105`](./archive/105-vision-governance-operationalization-strict-lane.md)
  archived after vision governance operationalization

Other planning specs:

- [`098-effigy-uninstall-command.md`](./098-effigy-uninstall-command.md) -
  draft top-level uninstall command scope and safety rules

## Next Task

Run the second governance review by 2026-09-17. Keep draft `098` paused until
the operator selects the next Horizon theme.
