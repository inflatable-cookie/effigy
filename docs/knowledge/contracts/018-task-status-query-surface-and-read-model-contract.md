# 018 - Task Status Query Surface And Read Model Contract

Owner: Platform (task engine)
Created: 2026-05-10
Rebuilt: 2026-10-19 on run records ([017](017-task-status-record-and-active-run-model-contract.md), [052](052-runtime-kernel-contract.md) §5.6 status)

Q-010 read model below is **proposed**, approved by planner and Tom (2026-10-22: "Approve") for
implementation with [052 §3.1](052-runtime-kernel-contract.md#31-q-010-run-classes-and-the-durability-boundary).
The existing binary still implements the row-based v2 envelopes. Phase 2
introduces the v3 envelopes specified here; this doc PR changes no JSON output.

## Purpose

`effigy tasks status` reports current custody and retained outcomes truthfully.
A task may run in foreground memory without creating a token, journal row or
completion record. The view therefore combines verified live gate evidence with
checkout-durable runs. History never stands in for current state, and age never
makes a live run stale. A missing row cannot prove the task never ran.

## Command surface

- `effigy tasks status <selector> [--json]`
- `effigy tasks status --all [--json]`

Both are pure reads (R2): they create, import and repair nothing. They may probe
an existing gate read-only, but never create a gate or acquire it exclusively.
`--all` enumerates tasks in the current checkout's effective catalogs, not the
host; cross-checkout discovery belongs to `runs --all` (052 §7.6).

## Selector resolution

`<selector>` resolves through the same resolver and directory rules as execution.
It names the same task/gates, including the filesystem-identity gate before the
first durable token exists (017; 052 §3.1). An ambiguous or unknown selector
returns the normal routing error; status never guesses a nearby record. An
absent checkout token does not short-circuit the live-gate probe or mint a token.

## Read model

Read the checkout journal if present and probe the task's existing owner gate(s).
Missing journal means no retained history, not no current run. A foreground
candidate must match the resolved task/root and a freshly verified holder's
boot/start identity; a PID or stale diagnostic body alone cannot create a live
row. The optional memory guard is also a custodian (052 §3.1): a held guard gate
is not a completed run merely because its invoking owner died. Without a live
owner response, detailed workload phase and outcome remain unavailable.

| Evidence | `state` and projection |
| --- | --- |
| Verified live foreground custodian, even with no token or database | `running`; `current` is a live diagnostic with provenance, verified pid and run id; phase, start time, profile and child steps are unavailable unless independently verified |
| Open durable run with matching held gate | `running` or `stopping`, from the admitted checkout run and fresh custody evidence |
| Non-closed durable run, gate free | `interrupted`; `current` is the unresolved durable run, with `next` naming its settle command (`effigy <task>`) |
| Closed durable run with unacknowledged incomplete cleanup | `failed-cleanup`; expose the recorded residue and recovery command, never a successful current outcome |
| Held gate whose diagnostics/identity cannot be verified, conflicting gate bindings, or unreadable evidence needed to resolve current custody | `unknown`, with the specific evidence warning; never `never-run` or a historical success |
| Positively discovered foreground residue without a live custodian | `unknown`, with evidence-limited residue diagnostics; no reconstructed run row, durable Interrupted state or numeric stop authority |
| Free/absent gates, no unresolved durable predecessor and no discovered residue | `not-recorded`; `current = null`; there is no retained foreground completion and no proof of the last invocation's outcome |

`latest` is the newest closed **durable** run, and `history` contains at most 20
durable runs, newest first. They remain available alongside a live foreground
`current` but are explicitly labelled recorded history. Foreground runs never
append either field. Unreadable history is reported as unavailable with a
warning; it cannot erase independently verified live foreground evidence.

With no live or unresolved run, even a closed durable success cannot prove it
was the latest invocation: an unrecorded foreground run may have followed it.
Thus the top-level state is `not-recorded`, with that durable result shown only
as `latest`/history. A reader cannot distinguish a task that never ran from one
whose foreground execution ended, so Q-010 stops emitting `never-run` from
absence alone. `run status <durable-id>` still reports that exact run's outcome.
This is a deliberate read-model change, not loss of durable history.

An exited foreground run leaves no durable Interrupted or cleanup record.
Free-gate diagnostic bytes never populate `current`, `latest` or `history`.
Any observed survivors are warnings with 052 §3.1's detection limits; absence
of readable tags is not proof of complete cleanup. v0.14 task status files
remain retired and are never read (017; 052 §10.1).

`--all` applies the same projection independently per declared task and includes
verified live diagnostics for no-longer-declared tasks when their identity is
readable (`currently_declared: false`). One unverifiable task contributes an
unknown row/warning, not a fabricated empty result for the checkout. Counts
include `not-recorded` and `unknown`; no reads create tokens, history or locks.

## JSON

The envelopes are `effigy.tasks-status.v3` and
`effigy.tasks-status-all.v3` (`schema_version = 3`). New absence semantics and
nullable live-diagnostic fields must not silently change the v2 contract.
Phase 2 updates the consumers and CLI checks together; there is no v2
compatibility shim that fabricates foreground history. Foreground inventory is
bounded to 64 gate-directory entries and 8192 resource entries; exceeding a
bound or reading unverified held-gate diagnostics produces explicit uncertainty.

| Field | Meaning |
| --- | --- |
| `task` | `selector` (runnable spelling), `catalog` (root-relative), `name`, `surface` |
| `state` | current-custody/availability state from the table above; closed outcomes live in recorded runs, not inferred current state |
| `current` | live foreground diagnostic or open/unresolved durable run; null when neither is established |
| current `evidence_source` | `live-foreground` or `checkout-journal`; never inferred from missing state |
| current fields | `run_id`, verified custodian `pid`; `phase`, `profile`, `started_at_epoch_ms` are nullable when unavailable; unavailable data is not copied from an older run |
| `latest` | newest closed durable run, with `evidence_source = "checkout-journal"`, phase/outcome, exit code, signal, cleanup and recorded timing; null if absent/unavailable |
| `history` | at most 20 durable runs newest first, with the same recorded shape; foreground diagnostics never enter history |
| run `steps` | verified direct child records recursively with `step` labels; omitted for foreground diagnostics without a verified owner response; omission does not prove no children |
| `next` | exact recovery command for an unresolved durable run; no invented replay command for lost foreground state |
| `warnings` | unreadable-state evidence, unavailable history or bounded foreground residue observations |
| `routing` | repo root, catalog alias, catalog root and manifest, selection mode and evidence |

The all-tasks envelope retains `scope_root`, `catalog_scopes`, `counts_by_state`,
`warnings` and rows with `selector`, `selected_catalog_root`, `currently_declared`,
`state`, `current`, `latest`, `next`, `warnings`. Its current/latest fields have
the same provenance and availability rules as the single-selector view.

## Text

One selector shows the task, current state, catalog and root. A live foreground
row says `running (foreground; completion not retained)` and names its verified
custodian/run id; unavailable phase, start and steps are stated as unavailable.
No live or unresolved run reads `not recorded (foreground completion is not
retained)`. A durable last result is separately labelled `last recorded durable
run`, with its exit code, duration, cleanup and recorded child steps. It is never
printed as the outcome of a newer foreground invocation. Unknown custody or
residue names its evidence limit; interrupted durable runs show their settling
command. `--all` groups the same projections by catalog.

## Proof obligations

Phase 2 must cover both commands in text and JSON: first live foreground run
without token/DB; live foreground after older durable success; normal exit;
owner/guard loss, stale/free gates and observable residue; unreadable diagnostics,
identity or history; interrupted/incomplete durable run; no retained runs; and
`--all` with declared and undeclared live tasks. Assert zero filesystem diff for
every read, no false `never-run` or stale current success, durable-only history,
v3 schema conformance and exact durable outcomes by run ID (052 §12.6).

## Drift triggers

Update this contract when projection rules, provenance/availability, the state
set, the history bound or either schema changes.
