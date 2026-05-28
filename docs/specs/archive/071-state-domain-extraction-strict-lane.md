# 071 - State Domain Extraction Strict Lane

Roadmap: [`g04.035`](../roadmaps/g04/035-state-domain-extraction.md)

Status: Complete
Owner: Platform
Created: 2026-05-12

## Purpose

Move stable state-stack domain behavior out of the runner so
`src/runner/state_command.rs` becomes orchestration code rather than the owner
of state reports, history, planning, and pure model rules.

## Hard Boundaries

- no state command grammar changes
- no state config format changes
- no JSON schema changes unless explicitly scoped by a later card
- no provider/deploy behavior changes
- no media/object-store implementation
- no Example App-specific transform or reconciliation logic
- no database rollback promises
- no `.github/workflows/` edits
- no release execution

## Worktree Boundary

The lane opens while `src/runner/state_command.rs` already has unrelated local
changes. Implementation must preserve those edits and avoid reverting them.

Before any state extraction card edits `state_command.rs`, the executor must
inspect the current diff and classify whether those edits are compatible with
the planned extraction slice.

## Ownership Boundary

`effigy-state` should own pure state-domain behavior:

- report model types that are not runner-specific
- report identity and path conventions
- lineage summaries
- plan construction that can run without side effects
- history inventory and latest-report selection
- blockers and warnings that are computed from config and recorded reports

Runner should keep side effects and command wiring:

- CLI dispatch
- manifest loading
- task invocation
- artifact staging
- SQL import execution
- hook execution
- final text/JSON output routing

## Execution Chain

- `662` complete: opened the lane, added the strict-lane and contract anchors,
  and selected the first classification card
- `663` complete: classified state command domain, adapter, rendering, and
  side-effect responsibilities
- `664` complete: moved state report path and history helpers into
  `effigy-state`
- `665` complete: moved pure apply plan builders into `effigy-state`
- `666` complete: moved pure state capture plan pieces into `effigy-state`
- `667` complete: closed the state runner thin-shell proof

## Exit Condition

This lane is complete when state report/history/planning ownership is materially
clearer, runner state code is smaller, command behavior remains compatible, and
the next media/object-store state seam is documented.

## Next Task

No active next task in this lane. Open `g04.036` manifest section decomposition.
