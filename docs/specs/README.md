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

- none; cards `1067` through `1069` completed `g08.026`

Queued strict lanes:

- none

Archived strict lanes:

- completed or paused historical strict lanes live in
  [`archive/`](./archive/)
- `076` through `096` are archived
- `097` and `099` are complete and awaiting archive on the next planning sweep

Other provisional spec:

- [`098-effigy-uninstall-command.md`](./098-effigy-uninstall-command.md) -
  draft top-level uninstall command scope and safety rules

## Next Task

Request explicit human authorization before release prepare. Keep the
unrelated `098` uninstall draft paused unless it is explicitly resumed.
