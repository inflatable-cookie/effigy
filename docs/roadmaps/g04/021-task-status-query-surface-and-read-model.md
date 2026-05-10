# 021 - Task Status Query Surface And Read Model

Generation: `g04`

Status: Queued
Owner: Platform
Created: 2026-05-10
Depends on:
- [`020-task-status-record-and-active-run-model.md`](./020-task-status-record-and-active-run-model.md)
- [`019-state-stack-and-layered-seed-framework.md`](./019-state-stack-and-layered-seed-framework.md)

## Goal

Expose a read-only operator surface for task status:

- `effigy tasks status <selector>`
- `effigy tasks status --all`

The command should show the active run when one is live and otherwise show the
last known outcome for the resolved task in the current repo context.

## Scope

- add `tasks status <selector>`
- add `tasks status --all`
- define text and JSON output contracts
- inventory the current repo root plus descendants from the current location
- merge live active records, latest completed records, and current task
  discovery into one read model
- surface undeclared stale records instead of silently dropping them

## Non-Goals

- no machine-wide or `--global` status scan
- no control verbs like stop, restart, unlock, or tail
- no built-in command status outside manifest-backed task execution
- no retention or cleanup policy for task-status history
- no second active strict lane while `g04.019` remains active
- no `.github/workflows/` edits
- no release execution

## Why This Lane Exists Separately

`020` is about status truth and write-side ownership.

`021` is about operator read semantics:

- routing
- inventory scope
- text readability
- JSON contracts
- unresolved/stale record presentation

Keeping them separate prevents the first implementation from mixing pipeline
truth with surface polish.

## Core Decisions

### Command Surface

Add:

- `effigy tasks status <selector>`
- `effigy tasks status --all`
- `--json`
- standard repo targeting through command-context rules

Do not add extra filter flags in the first round unless implementation forces
them.

### Selector Resolution

For `effigy tasks status <selector>`:

- resolve through the normal routing rules from the current repo context
- report status for the resolved selector in the selected scope
- include routing evidence in text/JSON so the operator can see what was
  anchored

If routing is ambiguous:

- use the normal task-routing failure family
- do not invent a weaker best-effort fallback

### `--all` Inventory Scope

`--all` inventories:

- the current repo root
- descendant catalogs/workspaces under that repo root

It must not scan outside the current repo boundary.

Inventory sources:

- known task selectors from task discovery/routing
- active task-status records under this repo scope
- latest completed records under this repo scope

If an active or latest record exists for a task no longer declared in the
current manifest set:

- include it
- mark it as unresolved / no-longer-declared
- do not silently drop it

### Text Output Shape

For one selector, show:

- resolved selector
- current state
- live stage, pid, elapsed time, and route/runtime summary when running
- otherwise last outcome, when, and duration
- selected catalog root / repo target
- profile/runtime detail when present
- warnings like stale active record ignored

For `--all`, group by selected catalog root or logical task scope.

Per row, show at minimum:

- selector
- state
- last updated timestamp
- short route/runtime summary
- unresolved/stale note when applicable

### JSON Contracts

Add dedicated schemas:

- `effigy.tasks-status.v1`
- `effigy.tasks-status-all.v1`

Single-selector payload should include:

- resolved selector
- selected catalog root
- state
- active record when live
- latest completed record when present
- warnings/evidence
- routing summary

`--all` payload should include:

- scope root
- discovered catalog scopes
- rows
- counts by state
- warnings/evidence

### Read-Side Ownership

The read path should live with task/runtime read ownership, not in CLI glue.

Likely ownership split:

- task-scope inventory and selector mapping: `effigy-tasks`
- active/completed record read and reconciliation: runtime/task-status read
  module
- final text and JSON shaping: dedicated report module

The CLI layer should only parse and dispatch.

## Public Interfaces Introduced

CLI:

- `effigy tasks status <selector> [--json]`
- `effigy tasks status --all [--json]`

JSON schemas:

- `effigy.tasks-status.v1`
- `effigy.tasks-status-all.v1`

Docs/reference updates:

- command reference matrix
- JSON output contracts guide
- troubleshooting/operator guide where status interpretation needs examples

## Acceptance Criteria

- `effigy tasks status <name>` shows active run when live and otherwise the
  last known outcome
- `effigy tasks status --all` inventories repo plus descendants from the
  current location
- undeclared-but-known stale records are visible
- text output is readable without JSON
- JSON contracts are stable and example-backed
- read behavior stays aligned with normal routing rules

## Suggested Batch Order

1. add parser/dispatch contract for `tasks status`
2. add single-selector read/report path
3. add `--all` repo-plus-descendant inventory
4. add JSON contracts and examples
5. update help, command reference, and troubleshooting guidance
6. close with proof matrix and contract promotion if stable

## Validation

- single-selector status tests for active, completed, unknown, and ambiguous
  routing cases
- `--all` inventory tests for repo-plus-descendant scope
- stale active-record warning tests
- undeclared/stale row visibility tests
- JSON contract tests and example fixtures
- docs path/link checks for changed roadmap/guide/contract surfaces
- `git diff --check`

## Next Task

After `020` stabilizes, make this the next active query-surface lane.
