# Batch Cards

Batch cards are the execution units for active Effigy strict-lane work.

## Working Rule

- one active ready card at a time
- completed cards must not remain advertised as ready
- if no card is ready, the lane is in planning
- keep the active tree focused on live or near-live cards
- archive stale cards once their lane is closed or paused cleanly
- do not use this index as a graveyard dump of historical cards

## Archive Boundary

Cards `430` and older have been archived under
[`../archive/batch-cards/`](../archive/batch-cards/).

The active batch-card tree now starts with `g04` rollover card `431`.

## Current Live Chain

- [`431-audit-runtime-architecture-and-open-g04.md`](./431-audit-runtime-architecture-and-open-g04.md)
  is complete. It landed the runtime architecture sanity audit and opened
  `g04`.

- [`432-scaffold-execution-pipeline-ownership-lane.md`](./432-scaffold-execution-pipeline-ownership-lane.md)
  is complete. It opened the first `g04.002` implementation lane and selected
  the dispatch-plan foundation slice.

- [`433-add-execution-dispatch-plan-foundation.md`](./433-add-execution-dispatch-plan-foundation.md)
  is complete. It added pure dispatch-plan types to `effigy-execution`.

- [`434-select-next-execution-planning-slice.md`](./434-select-next-execution-planning-slice.md)
  is complete. It selected the next bounded `g04.002` implementation slice.

- [`435-move-execution-preflight-input-behind-dispatch-plan.md`](./435-move-execution-preflight-input-behind-dispatch-plan.md)
  is complete. It moved runner preflight input behind the shared dispatch plan.

- [`436-select-discovery-or-selection-planning-slice.md`](./436-select-discovery-or-selection-planning-slice.md)
  is complete. It selected the next discovery or selection planning slice.

- [`437-add-execution-discovery-plan-foundation.md`](./437-add-execution-discovery-plan-foundation.md)
  is complete. It added the first shared discovery plan shape.

- [`438-select-selection-input-or-catalog-handoff-slice.md`](./438-select-selection-input-or-catalog-handoff-slice.md)
  is complete. It selected the next selection input or catalog handoff slice.

- [`439-add-execution-selection-plan-summary.md`](./439-add-execution-selection-plan-summary.md)
  is complete. It added the shared selection plan summary.

- [`440-select-binding-input-or-selected-task-adapter-slice.md`](./440-select-binding-input-or-selected-task-adapter-slice.md)
  is complete. It selected the next binding input or selected-task adapter
  slice.

- [`441-add-execution-binding-plan-summary.md`](./441-add-execution-binding-plan-summary.md)
  is complete. It added the shared binding plan summary.

- [`442-select-dispatch-stage-or-runtime-activation-handoff.md`](./442-select-dispatch-stage-or-runtime-activation-handoff.md)
  is complete. It selected closeout and runtime activation handoff.

- [`443-close-execution-pipeline-ownership-and-handoff-runtime-activation.md`](./443-close-execution-pipeline-ownership-and-handoff-runtime-activation.md)
  is complete. It closed `g04.002` and handed off to runtime activation.

- [`444-scaffold-runtime-activation-pipeline-lane.md`](./444-scaffold-runtime-activation-pipeline-lane.md)
  is complete. It scaffolded the `g04.003` runtime activation implementation
  lane.

- [`445-scaffold-effigy-runtime-plan-crate.md`](./445-scaffold-effigy-runtime-plan-crate.md)
  is complete. It added the first dependency-light runtime activation plan
  crate.

- [`446-select-first-runtime-plan-runner-integration.md`](./446-select-first-runtime-plan-runner-integration.md)
  is complete. It selected the first runner integration point for runtime
  planning.

- [`447-wire-runtime-activation-plan-into-exec-surface.md`](./447-wire-runtime-activation-plan-into-exec-surface.md)
  is complete. It wired runtime activation planning into `effigy exec`.

- [`448-select-next-runtime-activation-integration.md`](./448-select-next-runtime-activation-integration.md)
  is complete. It selected the next runtime activation integration point.

- [`449-wire-runtime-activation-plan-into-db-seed.md`](./449-wire-runtime-activation-plan-into-db-seed.md)
  is complete. It wired runtime activation planning into DB seed runtime prep.

- [`450-select-deferral-or-standard-task-runtime-integration.md`](./450-select-deferral-or-standard-task-runtime-integration.md)
  is complete. It selected deferral as the next activation integration point.

- [`451-wire-runtime-activation-plan-into-deferral.md`](./451-wire-runtime-activation-plan-into-deferral.md)
  is complete. It wired runtime activation planning into deferred container
  execution.

- [`452-wire-runtime-activation-plan-into-standard-task-activation.md`](./452-wire-runtime-activation-plan-into-standard-task-activation.md)
  is complete. It wired runtime activation planning into standard task
  activation.

- [`453-wire-runtime-activation-plan-into-managed-task-activation.md`](./453-wire-runtime-activation-plan-into-managed-task-activation.md)
  is complete. It wired runtime activation planning into managed task
  activation.

- [`454-select-runtime-prep-stage-migration-slice.md`](./454-select-runtime-prep-stage-migration-slice.md)
  is complete. It selected the first runtime-prep side-effect stage migration.

- [`455-move-runtime-prep-activation-executor-behind-plan.md`](./455-move-runtime-prep-activation-executor-behind-plan.md)
  is ready. It moves task activation side effects behind the activation plan.

## Next Task

Start card
[`455-move-runtime-prep-activation-executor-behind-plan.md`](./455-move-runtime-prep-activation-executor-behind-plan.md).
