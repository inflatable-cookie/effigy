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

- [`124`](./124-stale-local-install-recovery-strict-lane.md) — ready
  (`g09.009`, card `1117`)

Queued strict lanes:

- none

Archived strict lanes:

- every completed or paused strict lane lives in
  [`archive/`](./archive/); its README indexes them with closeout dates

Other planning specs:

- [`098`](./archive/098-effigy-uninstall-command.md) is paused historical
  planning; the shipped uninstall surface is not reopened by `g09.001`

## Next Task

Execute card `1117`, then archive spec `124`, close `g09`, and return to
Chatterbox for the operator-requested Northstar Refresh. Specs `118` through
`123` are archived. Effigy release authority stays separate.
