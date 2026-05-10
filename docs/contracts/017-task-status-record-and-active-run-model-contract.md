# 017 - Task Status Record And Active Run Model Contract

Status: Active
Owner: Platform
Created: 2026-05-10

## Purpose

Effigy needs one status truth model for repo tasks that survives beyond live
locks and caller-local runtime state.

The task-status contract sits above lock ownership and below any future
operator-facing `effigy tasks status` query surface. It defines what a task
status target is, how active versus completed records are represented, and how
stale active records are reconciled before they can be trusted.

## Scope

The task-status model owns:

- task-status identity across repo root, selected catalog root, and resolved
  selector
- normalized status state and running-stage taxonomy
- active status record layout under runtime state
- completed status record layout under report history
- stale/live reconciliation rules for active records
- shared write-side ownership through Effigy's execution pipeline

The task-status model does not own:

- final `effigy tasks status` CLI output shape
- machine-wide inventory
- control verbs such as stop, restart, tail, or repair
- status for non-task built-ins such as `doctor`, `release`, or `container`
- retention or pruning policy for task-status history

## Core Terms

Task status target:

- one resolved repo task identity
- keyed internally by repo root, selected catalog root, resolved selector, and
  optional resolved profile label

Task status key:

- the stable filesystem-safe identifier derived from task-status identity
- used for both runtime active-record storage and report-history storage
- deterministic across repeated executions of the same resolved task in the
  same selected scope

Active record:

- the runtime-state record for work Effigy currently believes is executing

Completed record:

- the latest plus append-only history record for work Effigy finished, blocked,
  or cancelled

Live record:

- an active record whose pid, heartbeat, and ownership evidence are still
  trustworthy

Stale active record:

- an active record that still exists on disk but no longer corresponds to a
  live Effigy-owned execution

## Status Identity Rules

The public query identity remains the selector an operator runs.

The internal status key must include:

- repo root
- selected catalog root
- resolved selector string
- optional resolved profile label when profile dispatch is involved

This is required so descendant catalogs with the same selector do not collide
in one repo.

The first-round implementation must derive the status key from normalized
identity fields rather than caller-local temporary paths.

The status key does not need to be human-friendly, but it must be:

- deterministic
- filesystem-safe
- collision-resistant across descendant scopes in one repo

## Normalized Status States

The first-round high-level state set is:

- `running`
- `succeeded`
- `failed`
- `cancelled`
- `blocked`
- `unknown`

`blocked` is reserved for bounded operator-visible failures where execution did
not truly begin, such as:

- lock conflict
- unsupported runtime route
- missing required runtime dependency

Any task that executed and ended unsuccessfully is `failed`.

## Running Stage Taxonomy

Active records may also carry a narrower stage value under `running`.

First-round stage set:

- `routing`
- `waiting_for_lock`
- `runtime_prep`
- `executing`
- `managed_session`
- `handoff`
- `finishing`

These stages refine `running`. They are not independent top-level states.

## Persistence Layout

Active records live under runtime state:

- `.effigy/runtime/tasks/active/<status-key>.json`

Completed records live under report history:

- `.effigy/reports/tasks/<status-key>/latest.json`
- `.effigy/reports/tasks/<status-key>/history/<timestamp>-<slug>.json`

The first round adds no retention or pruning policy.

## Minimum Active Record Fields

Active records must capture at least:

- status key
- normalized state, always `running`
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

The first round may add extra evidence fields, but it must not require the
later read/query lane to inspect arbitrary caller-local files to understand the
active task.

## Active Truth Rules

Effigy may present a task as actively running only when:

- an active record exists
- its pid or heartbeat evidence is still live enough to trust
- it still corresponds to real Effigy-owned execution

If an active record is stale:

- do not present it as `running`
- fall back to latest completed status when present
- surface stale-record evidence to the later query/report layer
- leave cleanup or repair to a later bounded lane

The first-round reconciliation order is:

1. load active record by status key
2. verify the recorded pid still exists
3. verify heartbeat/update freshness when heartbeat evidence is present
4. verify the record still maps to an Effigy-owned execution context when that
   evidence exists
5. only then trust the active record as live

If any required live check fails, the record is stale for status purposes even
if its file still exists on disk.

## Shared Write-Side Ownership

Task status must be written through the shared execution pipeline rather than
caller-local branches.

The first covered execution surfaces are:

- direct routed task execution
- managed task execution
- task-backed `dev` or managed-session entry
- deferred execution
- bootstrap delegated task execution
- run-array and Rhai embedded task re-entry through the shared request path
- watch-owned task runs, still keyed to the underlying selector

The first implementation slice may start at the canonical execution
request/plan boundary and does not need to hook every legacy caller-local path
independently if those paths already converge there.

## Minimum Record Fields

Completed records must capture at least:

- status key
- normalized state
- stage reached
- started and finished timestamps
- duration
- selector
- selected catalog root
- resolved task name
- resolved profile when present
- execution surface
- runtime route summary
- lock scopes
- outcome summary
- error family/code for `failed` and `blocked`
- written report paths

The first round may represent `TaskStatusRecord` as a shared envelope with
active/completed variants, or as sibling typed structs under one model. The
later query lane must not depend on caller-specific ad hoc JSON.

## First Implementation Slice

The smallest acceptable first implementation slice is:

- typed task-status key and record model
- runtime/report path helpers for active and completed record locations
- one shared writer owner on the canonical execution path
- write coverage for direct routed execution plus any other surfaces that
  already converge through the same path
- focused stale/live reconciliation helpers

This first slice does not need to add the final read/query CLI surface yet.

## Drift Triggers

Update this contract when any of these change:

- task-status identity key fields
- normalized state or stage taxonomy
- on-disk status record locations
- stale/live reconciliation rules
- execution surfaces that are covered by the shared writer
- minimum record fields expected by later read/query layers

## Validation

- focused task execution tests across routed, managed, bootstrap, deferred, and
  embedded surfaces
- active-record stale/live reconciliation tests
- descendant-scope collision tests
- docs path checks for roadmap/spec/contract front doors
