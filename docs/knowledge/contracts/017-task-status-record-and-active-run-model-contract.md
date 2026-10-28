# 017 - Task Status Record And Active Run Model Contract

Owner: Platform (task engine, Queue lane `v015-task-engine`)
Created: 2026-05-10
Rebuilt: 2026-10-19 on the runtime kernel ([052](052-runtime-kernel-contract.md) §5.1, §5.4, §5.6, §10.1)

Q-010 follows the storage and custody model approved by planner and Tom
(2026-10-22: "Approve") in [052 §3.1](052-runtime-kernel-contract.md#31-q-010-run-classes-and-the-durability-boundary).
Foreground evidence stays in memory; durable runs use checkout journals. The
foreground-aware task status surface uses v3 envelopes (contract 018).

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
durable history when recorded. Distinct tasks never collide: `a:b` and `a-b`
are two identities.
Identity never derives from the selector as typed.

Durable `CheckoutId` tokens keep the rules in 052 §5.1: linked worktrees and
ephemeral clones preserve their runtime-scope token, and primary checkouts
keep their token beside per-checkout Git metadata. First durable Prepare/Mutate
may mint a missing token. Foreground runs never mint one. Until a token exists,
they resolve a separate local gate identity from the verified root's canonical
path and its resolved per-checkout Git directory path (the root path alone for
non-Git roots), not a persisted token and not the path spelling the operator
typed (052 §7.1, effigy#337). Token initialization and both
gate names serialize as specified in 052 §3.1 and §7.1, so first durable use
cannot admit a second holder beside a foreground run.

Status resolves those same identities read-only. A missing token means no
persisted identity yet; it does not mean no live run or that the task never ran.
See 018 for verified foreground evidence and unrecorded completion.

## Exclusion (K2)

`Gate(Task)` on the owner, acquired by `Run::begin` before anything else
happens. A live custodian holds the gate: a foreground owner/memory guard
retains it through cleanup, while a durable owner uses the journal/sentinel
recovery path (052 §3.1, §5.8). Owner SIGKILL alone does not free a foreground
gate still held by its guard. A SIGSTOPped holder keeps it. Nothing steals,
reclaims or unlocks a gate.

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

Q-015 (Tom, 2026-10-27) adds one opt-in exception: `test:focused` keys its
exclusion by its resolved task plus the arguments its own command receives
(Q-015). A filter is the resolution: Effigy's flags are already split off, so
`--lock-wait-ms`, `EFFIGY_LOCK_WAIT_MS` and `--repo` never change the key, and
each argument is quoted canonically, so shell quoting does not either. Order
and repetition are kept, and no argument is dropped, so two different filters
stay two keys. An invocation with no task arguments keeps the declared task's
plain gate.

The second identical normalized invocation waits about two minutes
(`ARGUMENT_SCOPED_LOCK_WAIT_MS`) when neither `--lock-wait-ms` nor
`EFFIGY_LOCK_WAIT_MS` names a wait; an explicit value, `0` included, takes
precedence. Every other selector keeps the zero-wait default above. No
shared/read-only mode, lease, unlock path or host-wide coordination is added.

The run still records the declared `test:focused` identity, so its label, run
rows, logs and status read as that task; only the gate and journal key carry
the argument scope, and rerunning the same normalized invocation settles its
own predecessor (K5).

## Run record

The phase table below describes durable runs. Foreground runs keep admission,
intents and receipts in memory, with the same pre-go barrier and verified
process-group retention; the owner and memory guard acknowledge custody before
execution (052 §3.1, I-P2/I-P3). They create no journal rows or retained outcomes.
A lost owner/guard is not a durable `interrupted` record; survivors are
observational residue and grant no numeric signalling authority. Foreground
status is defined in 018, including the no-token first-run case. These durable
run projections populate recorded run fields; 018 defines the top-level task
state separately so a closed result never masks a later unrecorded invocation.

| Phase | Gate | Recorded run state |
| --- | --- | --- |
| `admitted`, `starting`, `active` | held | `running` |
| `stopping` | held | `stopping` |
| `closed`, complete cleanup | free | `succeeded`, `failed`, `cancelled`, `timed-out`, `not-started` or `interrupted` |
| `closed`, incomplete cleanup | free | `failed-cleanup` |
| not closed | free | `interrupted`; rerunning the task settles it first (K5) |
| unreadable | any | `unknown` |

- `Admitted` commits before the first effect, including env-schema `exec()`
  commands (K3), durably for durable runs and in memory for foreground runs.
  A run refused by a live holder executes none of them.
- A durable host command runs under kernel custody: its own process group, a
  durable custody record before it executes, a per-run sentinel that settles
  the run if the owner is SIGKILLed, and a settle that verifies the group
  closed. INT, TERM and HUP reach the group through the one signal router;
  the owner exits 128 + n with the run recorded `cancelled`.
- A durable workload outcome is recorded before settle; foreground outcomes
  exist only in the living run and the invoking stream. A failed journal write
  is a warning on stderr and never changes the task's exit code (K10).
- Retention keeps the newest 20 settled durable runs per task (K15).
  Foreground completion contributes no `latest` or `history` entry.
- Durable task runs live in `<checkout>/.effigy/state/kernel.db`; foreground
  runs use checkout-local lock files and memory only. Task runs never use the
  machine store. Managed and QA-group runs remain checkout-durable even when
  attached; caller-keyed runs select durability before effects (052 §3.1).

## Not owned here

- `effigy tasks status` output: [018](018-task-status-query-surface-and-read-model-contract.md).
- Host sequence steps are child kernel runs with independently gated custody.
  Parent settlement closes children first. Durable child rows record step
  labels and outcomes; foreground child effects remain in memory. JSON and text
  share execution. Container steps are child runs too, with their workload under remote custody (052 §5.10, effigy#236).
  Container setup embedded in lifecycle-managed process commands retains its
  readiness and startup order; its process exit remains the available outcome
  until the run supervisor takes custody of managed processes.
- Managed (`dev`) run supervision: the run supervisor lane. Managed runs hold
  the same task gate for their whole lifetime and record their run token's
  digest on the run. Each managed process receives the token through its spawn
  environment, never the supervisor's global environment (K12). The token marks
  ownership and grants nothing on its own, so process logs do not mask it.

## Migration

v0.14 status records (`.effigy/runtime/tasks/`, `.effigy/reports/tasks/`) are
retired in v0.15: never imported, read or rewritten. Q-010 status uses live
verified gate evidence plus checkout-durable rows (018); missing rows read
`not-recorded`, not `never-run`, regardless of old v0.14 status files. Old
status history is dropped, not state: nothing restarts, unlocks or settles
from it. `doctor` lists the v0.14 families still present;
they are left untouched.

## Drift triggers

Update this contract when `TaskId` fields, the checkout token rule, gate
semantics, the phase-to-state table, custody for plain commands, the
lock-conflict shape or retention change.
