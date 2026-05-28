# 035 - State Domain Extraction

Generation: `g04`

Status: Complete
Owner: Platform
Created: 2026-05-12
Depends on:
- [`034-shared-database-target-resolution.md`](./034-shared-database-target-resolution.md)

## Goal

Move state-stack domain behavior out of the runner so `state_command` becomes a
thin orchestration shell over `effigy-state`.

## Evidence

- `src/runner/state_command.rs` is 1,877 lines
- `effigy scan god-files --json` flagged it as a warning
- the runner currently owns too much plan, apply, capture, history, report,
  rendering, task, artifact, and SQL-import behavior
- future Example App migration and media capture work will add pressure to this
  surface if it remains runner-local

## Scope

- map state runner responsibilities into domain, adapter, and rendering groups
- move stable models and pure planning into `effigy-state`
- move report identity, report paths, and history read/write helpers into
  `effigy-state` where practical
- keep external command behavior stable
- leave impure execution adapters in runner until their boundaries are explicit
- prepare for media/object-store layers without implementing them here

## Non-Goals

- no new state commands
- no state config format changes
- no provider/deploy behavior changes
- no Example App transformation logic
- no automatic database rollback

## Target Boundaries

### `effigy-state`

Owns:

- state config model validation
- stack lineage summaries
- apply/capture plan construction
- report model types
- report path conventions
- history inventory and latest-report selection
- blockers and warnings that can be computed without side effects

### Runner

Owns:

- CLI dispatch
- manifest loading
- task invocation
- artifact staging side effects
- SQL import side effects
- text/JSON output selection

## Acceptance Criteria

- `src/runner/state_command.rs` is materially smaller and easier to navigate
- state report and history behavior is tested at the domain layer
- runner tests cover orchestration, not duplicated domain validation
- command output remains compatible
- roadmap notes identify the next media/object-store state seam

## Outcome

- moved state report path conventions into `effigy-state`
- moved state history scanning and classification into `effigy-state`
- moved apply report planning into `effigy-state`
- moved capture mode and produced-layer planning into `effigy-state`
- kept side effects, context writes, hooks, SQL import, artifact work, and text
  rendering in the runner
- preserved existing apply-hook worktree changes while extracting pure domain
  behavior

## Suggested Batch Cards

- `662-open-state-domain-extraction-lane.md`
- `663-classify-state-command-domain-and-adapter-responsibilities.md`
- `664-move-state-report-models-and-paths-into-effigy-state.md`
- `665-move-state-plan-builders-into-effigy-state.md`
- `666-move-state-capture-plan-pieces-into-effigy-state.md`
- `667-close-state-runner-thin-shell-proof.md`

## Validation

- `effigy-state` tests
- targeted state command tests
- `effigy state plan <fixture> --json`
- `effigy state history <fixture> --json`
- `effigy scan god-files --json`
- `git diff --check`

## Next Task

Open `g04.036` manifest section decomposition.
