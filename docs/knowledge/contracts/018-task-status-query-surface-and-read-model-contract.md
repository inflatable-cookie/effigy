# 018 - Task Status Query Surface And Read Model Contract

Owner: Platform (task engine)
Created: 2026-05-10
Rebuilt: 2026-10-19 on run records ([017](017-task-status-record-and-active-run-model-contract.md), [052](052-runtime-kernel-contract.md) §5.6 status)

## Purpose

`effigy tasks status` answers "is this task running, and how did it last
end?" truthfully. It projects the task's run rows plus a fresh probe of its
gate. History never stands in for current state, and age never makes a live
run stale.

## Command surface

- `effigy tasks status <selector> [--json]`
- `effigy tasks status --all [--json]`

Both are pure reads (R2): they create, import and repair nothing, and never
acquire a gate.

## Selector resolution

`<selector>` resolves through the same resolver as execution, from the same
directory rules, so it names the same `TaskId` the run used. An ambiguous or
unknown selector returns the normal routing error; status never guesses a
nearby record.

## Read model

For the resolved task, newest run first (at most 20):

- the newest run is not closed and its gate is held: `running` (or
  `stopping`), with the holder's pid;
- the newest run is not closed and its gate is free: `interrupted`, with
  `next` naming the command that settles it (`effigy <task>`);
- the newest run is closed: its recorded outcome;
- no runs: `never-run`;
- the run journal or checkout identity is unreadable: `unknown`, with a
  warning.

`current` is the newest run while it is open. `latest` is the newest closed
run. A live run is never replaced by an older outcome.

A task with no runs shows its v0.14 `latest` record, read-only and marked
`legacy`, for one minor.

## JSON

Single selector: `effigy.tasks-status.v2`.

| Field | Meaning |
| --- | --- |
| `task` | `selector` (runnable spelling), `catalog` (root-relative), `name`, `surface` |
| `state` | the state above |
| `current` | the open run: `run_id`, `phase`, `pid`, `profile`, `started_at_epoch_ms` |
| `latest` | the newest closed run: adds `exit_code`, `signal`, `cleanup`, `finished_at_epoch_ms`, `duration_ms` |
| `history` | newest first, the same run shape |
| `next` | the settling command for an interrupted run |
| `legacy` | the v0.14 record when the task has no runs |
| `warnings` | unreadable-state evidence |
| `routing` | repo root, catalog alias, catalog root and manifest, selection mode and evidence |

`--all`: `effigy.tasks-status-all.v2`, with `scope_root`, `catalog_scopes`,
`counts_by_state`, `warnings` and one row per declared task (`selector`,
`selected_catalog_root`, `state`, `currently_declared`, `current`, `latest`,
`next`, `warnings`). A live run of a task no longer declared is a row with
`currently_declared: false`.

## Text

One selector shows the task, state, catalog and root; the open run's id, phase,
start and pid; the settling command for an interrupted run; and the last run's
outcome, exit code, duration and cleanup. `--all` groups rows by catalog.

## Drift triggers

Update this contract when the projection rules, the state set, the history
bound, the legacy view or either schema changes.
