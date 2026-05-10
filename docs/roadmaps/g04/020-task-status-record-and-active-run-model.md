# 020 - Task Status Record And Active Run Model

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-10
Depends on:
- [`019-state-stack-and-layered-seed-framework.md`](./019-state-stack-and-layered-seed-framework.md)
- [`013-task-execution-request-contract.md`](../../contracts/013-task-execution-request-contract.md)
- [`009-execution-surface-convergence.md`](../../contracts/009-execution-surface-convergence.md)

## Goal

Define one canonical task-status record model and write-side pipeline so Effigy
can later answer:

- what task is actively running now
- what the last known outcome for a task was
- whether the live record is stale, blocked, or trustworthy

This lane is about status truth, not the final operator query UX.

## Scope

- define the task-status identity model
- define the normalized state and stage taxonomy
- define active versus completed status record layouts
- persist active records under runtime state and completed records under report
  history
- write task status through the shared execution pipeline rather than caller
  branches
- reconcile stale active records against pid / heartbeat truth
- prove the write path across routed, managed, bootstrap, deferred, and
  embedded task execution surfaces

## Non-Goals

- no final `effigy tasks status` command UX in this lane
- no restart / stop / tail / control verbs
- no machine-wide inventory
- no built-in command status for `doctor`, `release`, `container`, or other
  non-task command families
- no retention or pruning policy for task-status history
- no `.github/workflows/` edits
- no release execution

## Why Now

Effigy already has the ingredients for task status, but not one owner:

- task locks already encode owner pid, heartbeat, and remediation
- execution requests already preserve repo and runtime authority
- managed and bootstrap paths already converge through shared execution seams
- state-stack work already proved the `latest` plus `history/` report posture

What is still missing is one typed record model and one write path.

Without that, any future `tasks status` surface would either:

- degrade into lock inspection
- duplicate caller-local runtime state
- or silently disagree across routed, managed, bootstrap, and embedded flows

## Core Decisions

### Status Target Identity

A task status target is keyed by:

- repo root
- selected catalog root
- resolved selector string
- optional resolved profile label when profile dispatch is involved

The public query identity remains the selector the operator runs. The internal
status key must include catalog-root authority so descendant scopes do not
collide.

The first implementation should derive a deterministic filesystem-safe status
key from normalized identity fields rather than from caller-local temp paths or
runtime-only ids.

### Status Model

Task status is a merged view of:

- live active record, when present and trustworthy
- otherwise latest completed record
- otherwise no known status

Normalized high-level states:

- `running`
- `succeeded`
- `failed`
- `cancelled`
- `blocked`
- `unknown`

`blocked` is reserved for bounded operator-facing failures where execution did
not truly begin, such as:

- lock conflict
- unsupported runtime route
- missing required runtime dependency

Everything that executed and ended unsuccessfully is `failed`.

### Stage Model

Active records carry a narrower stage label under `running`.

Initial stage set:

- `routing`
- `waiting_for_lock`
- `runtime_prep`
- `executing`
- `managed_session`
- `handoff`
- `finishing`

### Persistence Layout

Use the existing runtime-state plus report-history posture.

Active records:

- `.effigy/runtime/tasks/active/<status-key>.json`

Completed records:

- `.effigy/reports/tasks/<status-key>/latest.json`
- `.effigy/reports/tasks/<status-key>/history/<timestamp>-<slug>.json`

No retention policy in the first round.

Minimum active record fields:

- status key
- `running` state
- current stage
- repo root
- selected catalog root
- resolved selector
- resolved task name
- resolved profile when present
- execution surface
- runtime route summary
- owner pid
- started timestamp
- last heartbeat/update timestamp
- lock scopes when known
- active record path

### Active Truth Rules

A task is only considered actively running when:

- an active record exists
- its pid / heartbeat is still live enough to trust
- it still corresponds to real Effigy-owned execution

If an active record is stale:

- do not present it as `running`
- fall back to latest completed status if present
- surface a warning/evidence note for the later query surface
- leave cleanup or repair to a later bounded follow-up

The first-round reconciliation order is:

1. load active record by status key
2. verify recorded pid liveness
3. verify heartbeat/update freshness when heartbeat evidence exists
4. verify Effigy-owned execution evidence when available
5. trust the record as live only when those checks pass

### Write-Side Ownership

The canonical writer hangs off the shared execution pipeline, not individual
callers.

The first round must cover:

- direct routed task execution
- managed task execution
- `dev` / managed session entry when task-backed
- deferred execution
- bootstrap delegated task execution
- run-array and Rhai embedded task re-entry through the shared request path
- watch-owned task runs, still keyed to the underlying selector

### Outcome Capture

Completed records must capture at least:

- normalized state
- stage reached
- started and finished timestamps
- duration
- selector
- selected catalog root
- resolved task name
- resolved profile if present
- execution surface
- runtime route summary
- lock scopes
- outcome summary
- error family/code for `failed` and `blocked`
- written report paths

## Public Interfaces Introduced

Internal typed model only in this lane.

Add a new internal contract surface around:

- `TaskStatusKey`
- `TaskStatusRecord`
- `TaskStatusState`
- `TaskStatusStage`
- `TaskStatusOutcome`
- `TaskStatusWarning`

No final user-facing command contract is introduced here.

## Acceptance Criteria

- one canonical task-status record model exists
- active and completed record layouts are explicit and documented
- task-shaped execution surfaces write through one shared status writer
- stale active records do not falsely present as running
- status identity does not collide across descendant catalogs
- focused proof coverage exists for routed, managed, bootstrap, deferred, and
  embedded task execution

## Suggested Batch Order

1. promote the status-record boundary and persistence layout
2. add typed record model and keying rules
3. wire shared writer hooks into the execution pipeline
4. add stale/live reconciliation helpers
5. land focused proof coverage
6. promote the contract if the first round is stable

## Validation

- focused task execution tests across routed, managed, bootstrap, deferred, and
  embedded surfaces
- status-record collision tests for descendant catalogs
- stale active-record reconciliation tests
- docs path/link checks for changed roadmap/contract surfaces
- `git diff --check`

## Next Task

Execute card
[`621-add-task-status-active-record-liveness-and-stale-reconciliation-helpers.md`](./batch-cards/621-add-task-status-active-record-liveness-and-stale-reconciliation-helpers.md).
