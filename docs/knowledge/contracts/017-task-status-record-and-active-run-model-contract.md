# 017 - Task Status Record And Active Run Model Contract

Owner: Platform (task engine, Queue lane `v015-task-engine`)
Created: 2026-05-10
Rebuilt: 2026-10-19 on the runtime kernel ([052](052-runtime-kernel-contract.md) §5.1, §5.4, §5.6, §10.1)

## Purpose

One identity, one exclusion and one status truth for every repository task
run. A task run is a kernel `Run`; its owner is the task's `TaskId`; holding
the owner gate is the run's liveness. There is no lease, heartbeat,
staleness window, unlock command or task cache.

## Task identity (K1)

The resolver produces one `TaskTarget` per invocation. Its kernel `TaskId` is:

| Field | Value |
| --- | --- |
| checkout | the root's `CheckoutId` (below) |
| catalog | the selected catalog's path relative to the resolved root, `/`-separated; `""` for the root catalog |
| surface | `published` or `draft` |
| task | the declared task name |
| profile | a managed profile, recorded on the run; the gate stays per task |

Every spelling that selects the same declared task (`build` from inside the
catalog, `api/build`, `./apps/api/build`) gives one `TaskId`, one gate and one
history. Distinct tasks never collide: `a:b` and `a-b` are two identities.
Identity never derives from the selector as typed.

`CheckoutId`:

- linked worktree or ephemeral clone: its runtime-scope token, so Compose names
  do not change;
- primary Git checkout: `.git/effigy-checkout-id`, minted on the first run;
- a root with no `.git` of its own: derived from its canonical path.

Only a run (Mutate) mints a token; status reads it and reports "no runs yet"
when it is absent.

## Exclusion (K2)

`Gate(Task)` on the owner, acquired by `Run::begin` before anything else
happens. It is held exactly while the owning Effigy process lives, and the
kernel releases it when that process exits for any reason, SIGKILL included.
A SIGSTOPped holder keeps it. Nothing steals, reclaims or unlocks a gate.

- `--lock-wait-ms <MS>` (before the task name) or `EFFIGY_LOCK_WAIT_MS`
  bounds the wait; omitted or `0` fails at once.
- `lock = "<name>"` adds a shared lock: a `Store(task-lock:<name>)` resource
  held by its own run around the task's run, so every task declaring the name
  excludes the others.
- A refused admission writes nothing. Its error (`effigy.lock-wait.v2` in JSON
  details, exit 75) names the holder from the gate probe: pid on this host, run
  id and age, and `stopped` for a SIGSTOPped holder. It points at
  `effigy tasks status <task>` and `--lock-wait-ms`, never at breaking the lock.
- A v0.14 lease record (`.effigy/locks/*.lock`) whose holder is still alive on
  this host refuses its task until that process exits. Dead, foreign or
  corrupt records are ignored and never deleted; `doctor` lists them.

## Run record

| Phase | Gate | Status state |
| --- | --- | --- |
| `admitted`, `starting`, `active` | held | `running` |
| `stopping` | held | `stopping` |
| `closed`, complete cleanup | free | `succeeded`, `failed`, `cancelled`, `timed-out`, `not-started` or `interrupted` |
| `closed`, incomplete cleanup | free | `failed-cleanup` |
| not closed | free | `interrupted`; rerunning the task settles it first (K5) |
| unreadable | any | `unknown` |

- `Admitted` commits before the first effect, including env-schema `exec()`
  commands (K3). A run refused by a live holder executes none of them.
- A plain host command runs under kernel custody: its own process group, a
  durable custody record before it executes, a per-run sentinel that settles
  the run if the owner is SIGKILLed, and a settle that verifies the group
  closed. INT, TERM and HUP reach the group through the one signal router;
  the owner exits 128 + n with the run recorded `cancelled`.
- The workload outcome is recorded before settle. A failed journal write is a
  warning on stderr and never changes the task's exit code (K10).
- Retention keeps the newest 20 settled runs per task (K15).
- Runs live in the kernel store, `~/.effigy/state/kernel.db`. Nothing about
  a task run is written into the checkout.

## Not owned here

- `effigy tasks status` output: [018](018-task-status-query-surface-and-read-model-contract.md).
- Sequence step journals and the one sequence engine: the follow-up task engine
  work.
- Managed (`dev`) run supervision: the run supervisor lane. Managed runs hold
  the same task gate for their whole lifetime and record their run token's
  digest on the run. Each managed process receives the token through its spawn
  environment, never the supervisor's global environment (K12). The token marks
  ownership and grants nothing on its own, so process logs do not mask it.

## Migration

v0.14 status records (`.effigy/runtime/tasks/`, `.effigy/reports/tasks/`) are
never imported or rewritten. For one minor, `effigy tasks status` shows a
task's v0.14 latest record read-only until the task first runs. `doctor`
lists the v0.14 families still present.

## Drift triggers

Update this contract when `TaskId` fields, the checkout token rule, gate
semantics, the phase-to-state table, custody for plain commands, the
lock-conflict shape or retention change.
