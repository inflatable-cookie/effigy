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
- [`101`](./archive/101-explicit-catalog-membership-strict-lane.md) is archived
  after completing explicit catalog membership
- [`102`](./archive/102-unified-test-orchestration-v011.md) is archived after
  completing v0.11 unified test orchestration
- [`103`](./archive/103-pre-release-ci-proof.md) is archived after making
  exact-candidate hosted CI proof release-blocking
- [`104`](./archive/104-bun-committed-dependency-pinning.md) is archived after
  completing committed Bun pinning, the consumer proof, and the bounded
  `InvalidPackageInfo` follow-up
- `097`, `099`, and `100` are complete and awaiting archive on the next
  planning sweep

Other planning specs:

- [`098-effigy-uninstall-command.md`](./098-effigy-uninstall-command.md) -
  draft top-level uninstall command scope and safety rules

## Next Task

No ready strict card remains. Keep the unrelated `098` uninstall draft paused
until the operator selects the next planning direction.
