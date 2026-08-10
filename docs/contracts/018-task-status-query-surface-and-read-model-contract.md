# 018 - Task Status Query Surface And Read Model Contract

Status: Active
Owner: Platform
Created: 2026-05-10

## Purpose

Effigy needs one read-side contract for task status so the later
`effigy tasks status` surface can answer operator questions without weakening
task routing rules or silently dropping stale status truth.

This contract sits above the persisted task-status record model from `017` and
below the final CLI/help/docs layer. It defines the read-side scope, selector
resolution rules, and the minimum text/JSON result shape the query surface must
honor.

## Scope

The task-status query contract owns:

- selector-based read semantics for `effigy tasks status <selector>`
- repo-scoped inventory semantics for `effigy tasks status --all`
- merging live active records, latest completed records, and current task
  discovery into one read model
- visibility rules for stale or no-longer-declared records
- minimum single-selector and `--all` JSON contract fields

The task-status query contract does not own:

- machine-wide or `--global` status inventory
- control verbs such as stop, restart, unlock, or tail
- retention or cleanup policy for task-status history
- status for non-task built-ins such as `doctor`, `release`, or `container`

## Command Surface

The first read-side surface is:

- `effigy tasks status <selector>`
- `effigy tasks status --all`
- `--json`
- standard repo targeting through normal command-context rules

The first round adds no extra filter flags beyond `--all`.

## Selector Resolution Rules

For `effigy tasks status <selector>`:

- resolve through the same routing rules as normal task execution
- return status for the resolved selector in the selected catalog scope
- include routing evidence so the operator can see what scope won

If routing is ambiguous:

- use the normal task-routing failure family
- do not invent a best-effort fallback

If the selector does not resolve:

- return the normal not-found failure family
- do not fabricate an `unknown` status row for a missing task

For one-selector queries, there is no fallback to “closest known status record”
when routing fails. The read surface must stay aligned with execution routing.

## `--all` Inventory Scope

`--all` is repo-local. It inventories:

- the current repo root
- descendant catalogs/workspaces under that repo root

It must not scan outside the current repo boundary.

Inventory sources are:

- known task selectors from current task discovery
- active task-status records under this repo scope
- latest completed task-status records under this repo scope

If a status record exists for a selector no longer declared in the current
manifest set:

- keep it visible
- mark it as unresolved / no-longer-declared
- do not silently drop it

If a selector is currently declared but has never run and has no persisted
record yet:

- include it in `--all`
- classify it as `unknown`
- keep it distinct from stale/no-longer-declared rows

## Read Model

The query surface merges status truth in this order:

1. trusted live active record when present
2. otherwise latest completed record
3. otherwise no known status

The read model must surface:

- stale active record warnings/evidence from the `017` reconciliation layer
- whether the row still maps to a currently declared task
- the selected catalog scope for the row

For one-selector queries:

- if a stale active record exists and a latest completed record exists, return
  the latest completed record plus warnings/evidence about the stale active
  record
- if a stale active record exists and no completed record exists, return
  `unknown` plus the stale-record warnings/evidence

`unknown` is reserved for rows the read model knows about but cannot currently
classify beyond “no trusted active record and no completed record”.

## Text Output Rules

For one selector, text output must show at least:

- resolved selector
- current state
- live stage, pid, elapsed time, and route/runtime summary when running
- otherwise last outcome, when, and duration
- selected catalog root / repo target
- warnings like stale active record ignored

For `--all`, text output must:

- group by selected catalog root or logical task scope
- show one row per selector with state, last update, short route/runtime
  summary, and unresolved/stale notes when present

## JSON Schemas

The first read-side schemas are:

- `effigy.tasks-status.v1`
- `effigy.tasks-status-all.v1`

Single-selector payload must include at least:

- resolved selector
- selected catalog root
- resolved/current state
- trusted active record when live
- latest completed record when present
- stale active record warnings/evidence when present
- warnings/evidence
- routing summary

`--all` payload must include at least:

- scope root
- effective catalog scopes
- rows
- counts by state
- warnings/evidence

Each `--all` row must include at least:

- selector
- selected catalog root
- state
- currently-declared boolean
- trusted active record when live
- latest completed record when present
- unresolved/no-longer-declared marker when applicable
- stale active warnings/evidence when applicable

## Read-Side Ownership

The read path should stay below CLI glue.

Expected ownership split:

- task inventory and selector mapping: task discovery/routing layer
- active/latest record load and stale reconciliation: runtime task-status read
  module
- text/JSON shaping: dedicated report module

The CLI layer should only parse and dispatch.

## Drift Triggers

Update this contract when any of these change:

- `tasks status` selector resolution rules
- repo-plus-descendant `--all` scope rules
- visibility rules for stale or undeclared records
- minimum text result content
- JSON schema ids or minimum fields
- read-side ownership boundary between runtime/task discovery/report layers
