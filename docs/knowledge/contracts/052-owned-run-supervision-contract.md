# 052 - Owned Run Supervision Contract

Status: proposed and unavailable. No run journal, run-scoped stop, run-scoped
logs, signal-evidence record, or hard-timeout enforcement described here
exists today. Until an implementation brief lands, use existing selector
status, managed-session controls, and `effigy admission` queries; never claim
scoped stop or signal attribution from this document.

The scheduler ownership ruling in [049](049-heavy-validation-admission-contract.md)
governs the next implementation cut. Queue and Nucleus own admission and the
scheduler-launched process group; Effigy owns execution, graceful termination,
exit evidence and runtime telemetry. This proposed design must be reconciled
with their agreed shared contract before implementation. Its lease-store and
supervisor-placement proposals do not authorize extending the current admission
mechanism.

Owner: task execution and lifecycle maintainers
Architecture: [032](../architecture/032-owned-run-supervision-runtime.md)
Workflow: [082](../../guides/082-owned-run-supervision-workflow.md)
Contracts: [013](013-task-execution-request-contract.md),
[017](017-task-status-record-and-active-run-model-contract.md),
[018](018-task-status-query-surface-and-read-model-contract.md),
[046](046-published-and-draft-task-surface-contract.md),
[049](049-heavy-validation-admission-contract.md),
[051](051-bounded-qa-groups-contract.md)

## Purpose and proof boundary

Effigy needs one supervision boundary that can stop one owned run and state
what was actually observed about its interruption. Today signal forwarding,
process-group registration, and crash recovery exist only inside the heavy
admission lease; ordinary tasks, nested children, and QA-group members have no
run-scoped stop and no honest signal record. Contracts
[049](049-heavy-validation-admission-contract.md) and
[051](051-bounded-qa-groups-contract.md) already refuse to claim those controls
until this contract and its implementation land.

This contract defines, for one owned run:

- the run identity, process generation, and ownership evidence a stop may act
  on;
- supervisor placement and lifetime for heavy, non-heavy, and group runs;
- the ordered cancellation, cleanup, lease-release, and recovery sequence;
- the exact status, logs, stop, and capability surfaces and JSON;
- the signal/interruption evidence taxonomy and its honest unknowns.

It is a supervision contract, not an execution contract. It does not run a
task, change the request/pipeline boundary of contract
[013](013-task-execution-request-contract.md), change selector status under
contracts [017](017-task-status-record-and-active-run-model-contract.md) and
[018](018-task-status-query-surface-and-read-model-contract.md), change
admission scheduling or lease reclamation under contract
[049](049-heavy-validation-admission-contract.md), or add group grammar under
contract [051](051-bounded-qa-groups-contract.md).

A stop proves only that the supervision boundary resolved one generation,
delivered the signals it recorded, and confirmed descendant closure where it
could prove it. It never proves that a run was killed by an unnamed third
party, never proves a missing child is gone without generation evidence, and
never fabricates a final receipt for a process that could not write one.

## Vocabulary

- **Run**: one Effigy invocation's execution, identified by a host-unique
  `run_id`. A QA-group run is one run; each member executes inside it.
- **Run generation**: the tuple `(boot_id, owner_pid, owner_start_identity,
  supervision placement, supervisor_pid, supervisor_start_identity, ordered
  supervised process groups, started_at)`. A PID is never an identity.
- **Supervised process group**: one child process group created by the
  canonical pipeline (`setpgid`), recorded with its leader start identity.
- **Supervision boundary**: the one component that owns a run generation,
  registers its groups, forwards cancellation, confirms closure, and writes
  interruption evidence. Two placements exist: `inline` and `detached`.
- **Stop request**: one operator, owner, supervisor, or timeout request to
  end a run generation.
- **Interruption evidence**: the recorded signals, signal observations, and
  closure proofs for a run. Absent evidence is `unknown`, never `none`.
- **Witness**: a process that survives an owner to record that the owner died.
- **Capacity waiter**: a run in `waiting_for_capacity` holding no process
  generation and no lease.
- **Canonical checkout**: the canonical absolute repository root of the owner
  invocation. Worktrees and clones are distinct checkouts.
- **Capability**: whether this platform and route can supervise, stop, and
  enforce a hard timeout with proof.

## Ownership and identity

Every run gets one durable record before it can spawn a child or acquire a
lease. The record is written through the host-wide coordinator that contract
[049](049-heavy-validation-admission-contract.md) already mandates, so all
Effigy processes on one host share one lock, one queue, and one run journal.

The record carries at least:

- `run_id`: opaque, host-unique, and **not** an authorization token;
- `kind`: `capacity_lease` (heavy, holds a lease) or `evidence_only` (does not
  consume capacity and is invisible to admission scheduling);
- `checkout`: canonical owner repository root; `catalog` and `selector`;
- `caller`: the opaque `EFFIGY_CALLER` identity or the local interactive
  identity, recorded as evidence only under contract
  [049](049-heavy-validation-admission-contract.md);
- `generation`: the tuple above, including supervisor placement and every
  supervised process group with its leader start identity;
- `state`, timing, exit classification, and the log reference.

### Boot and start generation

- `boot_id` reuses admission's existing boot identity (`/proc/sys/kernel/random/boot_id`
  on Linux, `kern.boottime` on macOS). A differing boot id invalidates the
  record.
- `owner_start_identity` and each group-leader start identity reuse admission's
  existing per-process start identity (`/proc/<pid>/stat` field 22 on Linux,
  `ps -o lstart=` on macOS).
- A process counts as the recorded process only when boot ids match **and** the
  live start identity equals the recorded one. A missing start identity on
  either side is uncertain, not a match.

### PID reuse

A live PID whose start identity differs from the record is a different
process. Effigy must never signal it, must never treat it as the recorded
owner or group leader, and must never declare the recorded generation gone
because of it. Reused PIDs keep the generation `unknown`, and a capacity
holder stays reserved under the unchanged admission rules.

### Process groups and nested descendants

- Each child is spawned in its own process group by the canonical pipeline and
  registered in the run generation immediately after `spawn()`, before its
  output is consumed or its side effects are relied on.
- At signal time the boundary enumerates the live descendants of a recorded
  group with the existing `ppid` walk and signals proven members, not a global
  pattern. A process that cannot be tied to a recorded generation is not
  signalled.
- Closure is proven per group: boot id matches, the group probe reports no
  member, and every recorded leader start identity no longer matches. An
  unambiguous `EPERM`, an unavailable `ps`, or a missing start identity is
  `unknown`.

### Canonical checkout, caller, and run authorization

- `run_id` selects a candidate record. It does not authorize control.
- **Read** (`status`, `logs`) is allowed to the recorded owner or supervisor,
  or to another process of the same effective OS user on the same host. The
  contract [049](049-heavy-validation-admission-contract.md) state-root
  ownership and permission model already bounds this; this contract adds no
  broader read authority.
- **Control** (`stop`) is allowed only to the recorded owner generation, the
  recorded supervisor generation, or a same-user process whose canonical
  checkout equals the record's `checkout`. A differing checkout or an
  unresolvable record checkout is refused with `refused_foreign_checkout` and
  no signal is delivered.
- A caller string, a selector, a PID, and a run id each have no stop authority
  on their own. Cross-user control never widens beyond the shared-state group
  posture already defined by contract
  [049](049-heavy-validation-admission-contract.md).

## Supervisor placement and lifetime

One run has exactly one supervisor at a time. Both placements share the same
journal schema, the same group registration, the same stop sequence, the same
capability probe, and the same authorization rules. This is the single
supervision boundary reused across heavy, non-heavy, and group runs.

### Inline (default)

The owner process is the supervisor. It installs cancellation forwarding and
the process-group observer at run start, forwards `SIGINT`/`SIGTERM`/`SIGHUP`
to its registered groups, and writes the final record and releases its lease
on normal exit, `SIGTERM`, or a stop request. Lifetime is the owner
invocation.

Inline cannot attest its own `SIGKILL`. If the owner is killed and no witness
exists, the run stays interrupted/`unknown` until reconciliation, and no final
receipt is invented.

### Detached (opt-in, required for owner-loss attribution)

A run that must attribute owner loss, enforce a hard timeout, or report
interruption after owner death declares `detached` supervision before
execution. The boundary starts one per-run witness that:

- lives in its own session/process group so the owner's `SIGKILL` cannot take
  it out, and observes the owner generation and the run journal independently;
- holds the run generation and its registered groups, records stop and
  timeout actions, and confirms closure with the same evidence rules;
- starts no task work and owns no task semantics. The canonical owner process
  stays the only executor; the witness never spawns, executes, or
  reinterprets a task. It is a witness and signal relay, not a second runner.

The witness proves closure only from recorded generations: boot id, group
leader start identities, and group probes. A descendant that was never
registered, or that was reparented beyond its recorded group, is `unknown`,
not gone. A route whose registered groups cannot be verified must not claim
detached attribution. A detached run whose witness is unavailable states
`unavailable_supervisor` and does not claim cancellation.

Exactly one boundary owns a generation. A placement handoff (for example
inline start followed by a detached witness takeover) must be recorded as one
generation with two supervisor lifetimes and must never produce two
simultaneous supervisors for the same groups.

## Run lifecycle

1. **Create**: write the run record with `kind`, `checkout`, `caller`,
   owner generation, and supervision placement before acquiring a lease,
   preparing setup, or spawning a child.
2. **Wait**: a heavy run may be `waiting_for_capacity` with no groups. A stop
   cancels the waiter (below). No supervisor signals exist yet.
3. **Admit**: a heavy run acquires its contract
   [049](049-heavy-validation-admission-contract.md) lease unchanged; an
   evidence-only run acquires nothing and never becomes a capacity holder.
4. **Supervise**: register each child group as it spawns; a group ledger
   records member boundaries for a QA-group run.
5. **Finish**: on completion, cancellation, timeout, or signal, confirm or
   classify closure, release only the run's own lease when closure holds,
   and persist the terminal record with interruption evidence.

A run record is process evidence, not a lock. Creating it never blocks another
run, and a stale record never grants authority over a live process.

## Cancellation, hard timeout, and waiter stop

### Ordered stop sequence

A stop request runs this order and records each step before it acts, so the
evidence survives owner loss:

1. Resolve the run id to one record; verify authorization and generation.
   Refuse a foreign checkout, an untrusted journal, or an unresolvable
   generation without signalling.
2. Persist the request: requester, source (`operator`, `owner`, `supervisor`,
   `timeout`), signal, grace, and timestamp.
3. If `waiting_for_capacity`: mark the waiter `cancelled`
   (`waiter_cancelled`), clear its position, write the terminal record, and
   stop. Never touch a lease owner.
4. Deliver the requested signal (default `SIGTERM`) to the proven owner
   generation and its proven supervised groups.
5. Wait up to the grace window, re-probing with generation evidence.
6. Escalate to `SIGKILL` only for members still proven to belong to the same
   generation. Never escalate against an unproven or foreign process.
7. Confirm descendant closure. If closure is unproven, the child state is
   `unknown`; do not declare it gone and do not kill it.
8. Release exactly the run's own contract
   [049](049-heavy-validation-admission-contract.md) lease, and only when
   closure holds. Never release another run's lease and never free capacity
   while a proven child is live.
9. Persist the terminal record: state, classification, per-signal evidence,
   timing, and log reference. The surviving owner or witness writes it; if
   neither survives, reconciliation writes an interrupted/`unknown` record
   and never a fabricated completion.

### Hard timeout

- A hard timeout is separate from `expected_wall_ms` and from the admission
  deadline. It starts after admission, not during capacity wait.
- It uses the same ordered stop sequence and records `timed_out`, distinct
  from `cancelled`, `capacity_timeout`, and a failed check.
- It is available only when the resolved run route can be supervised with
  closure proof. An unsupported route fails plan/capability validation before
  execution and never starts a run it cannot stop. It never pretends to
  enforce a timeout on an unowned child.
- If timeout closure is unproven, the outcome stays `timed_out` with child
  state `unknown`, and capacity stays reserved under the unchanged admission
  rules.

### Capability negotiation with the group runtime

Before a QA-group run starts, the group plan reads the capability result and
compares it with the run declaration:

- a declared `hard_timeout_ms` with `hard_timeout = false`, an unsupported
  platform, or an unsupervisable member route fails the plan with a typed
  `unsupported_run_control` reason and no run id;
- a `stop` request against a run whose placement reports
  `unavailable_supervisor` returns that reason and does not claim cancellation;
- the capability probe is read-only and acquires no capacity.

Contract [051](051-bounded-qa-groups-contract.md) already requires
`hard_timeout_ms` to stay unavailable until the runtime can enforce it for
every resolved route; this contract supplies that gate.

## Signal and interruption evidence

Record a signal only when the recording process has evidence. Never invent a
killer identity, never infer one from a shutdown policy, and never write a
final receipt from a dead owner.

| Situation | `interruption` | Evidence required |
| --- | --- | --- |
| Child signal observed by a surviving owner | `child_signalled` | The owner reaped or observed the child's signal termination (`ExitStatus::signal`) or proved the child gone after this run signalled it. `child_signal_observed` records pid, group, signal, observer, time. |
| Owner received `SIGTERM` | `owner_terminated` | The owner's own signal handler recorded `owner_received_signal`; forwarding and closure come from the ordered stop sequence. |
| Owner `SIGKILL` with a surviving supervisor | `owner_lost` | The supervisor proves the owner generation gone by start identity, records `owner_lost`, and reports each child as `gone`, `live`, or `unknown` from generation evidence. |
| No surviving witness | `unknown` | Nothing is claimed. The record stays interrupted/`unknown` until reconciliation; no completion, no cancellation receipt. |

`signal_delivered` entries exist only when the recording process actually
called `kill()` and observed the result. `owner_received_signal` exists only
from the owner's handler. A signal inferred from policy, from a name, or from
another process's exit without observation is not recorded as delivered.
Ambiguous evidence is `unknown`.

## Permissions and ambiguous evidence

- The journal reuses contract
  [049](049-heavy-validation-admission-contract.md) state-root validation:
  directory ownership, permissions, symlink rejection, and fail-closed
  behavior when the coordinator is untrusted or unavailable.
- Stop fails closed on an untrusted journal. It never falls back to a
  per-repository file, never reads an overlay, and never guesses.
- `EPERM`, an unavailable `ps`, a missing boot id, or a missing start identity
  resolves to `unknown`, never `gone` and never signalled.
- On platforms without the required process and session primitives,
  supervision capabilities are absent and stop/hard-timeout are rejected
  before execution.

## Persistence, restart, and recovery

- The journal writes atomically through the contract
  [049](049-heavy-validation-admission-contract.md) lock and temp+rename path.
- A differing boot id invalidates the generation. Reconciliation must not
  credit a final receipt to a dead process.
- After a restart, an interrupted run is reconciled from generation evidence:
  proven gone becomes terminal with its recorded classification; uncertain or
  live becomes `unknown`/`stale_owner_unknown`.
- A live foreign or uncertain child is never declared gone and never killed.
  Capacity stays reserved for an uncertain holder under the unchanged
  admission recovery rules. This contract does not change lease reclamation.

## Command surface and JSON

Run control is run-id scoped and lives under the reserved `tasks` family, so
no repository task selector is shadowed. Selector-scoped status under
contracts [017](017-task-status-record-and-active-run-model-contract.md) and
[018](018-task-status-query-surface-and-read-model-contract.md) is unchanged.
The group-facing `effigy tasks qa-group status|logs|stop <run-id>` forms from
contract [051](051-bounded-qa-groups-contract.md) resolve the same run ids
through the same resolver and schemas.

```text
effigy tasks run status <RUN_ID> [--json]
effigy tasks run logs <RUN_ID> [--follow] [--json]
effigy tasks run stop <RUN_ID> [--signal SIGTERM|SIGKILL] [--grace-ms N] [--json]
effigy tasks run capabilities [--json]
```

There is no PID, selector, or process-name argument. `--json` changes output
only. Text and JSON agree. Logs and evidence redact secret values.

### `effigy.run-status.v1`

```json
{
  "schema": "effigy.run-status.v1",
  "schema_version": 1,
  "run_id": "<run-id>",
  "kind": "capacity_lease",
  "state": "running",
  "outcome": null,
  "interruption": "none",
  "checkout": "<canonical-absolute-checkout>",
  "catalog": "<catalog-alias>",
  "selector": "<resolved-selector>",
  "caller": "<opaque-caller-or-interactive-identity>",
  "supervision": {
    "placement": "inline",
    "supervisor": {
      "pid": 1234,
      "start_identity": "<identity-or-null>",
      "state": "live"
    },
    "generation": {
      "boot_id": "<boot-id-or-null>",
      "owner_pid": 1234,
      "owner_start_identity": "<identity-or-null>"
    }
  },
  "groups": [
    {
      "pgid": 1240,
      "leader_start_identity": "<identity-or-null>",
      "state": "live",
      "signal_evidence": []
    }
  ],
  "capacity": {
    "lease_id": "<lease-or-null>",
    "reservation": { "cpu_units": 4, "memory_mib": 8192 },
    "admission_wait_ms": 1200,
    "wall_ms": 45000,
    "budget_state": "within_budget"
  },
  "signal_evidence": {
    "requested_by": null,
    "request_source": null,
    "delivered": [],
    "child_signal_observed": [],
    "owner_received": []
  },
  "timing": {
    "queued_at": "<time-or-null>",
    "admitted_at": "<time-or-null>",
    "started_at": "<time-or-null>",
    "ended_at": null,
    "execution_wall_ms": null
  },
  "log_ref": "<run-scoped-reference-or-null>",
  "evidence_complete": false
}
```

States are `waiting_for_capacity`, `running`, `succeeded`, `failed`,
`cancelled`, `timed_out`, `capacity_timeout`, `blocked`, `unknown`. A run
whose owner died without a witness reports `unknown` with
`evidence_complete = false` and `interruption = "unknown"`. Null timing means
unavailable, not zero. Group `state` is `live`, `gone`, or `unknown`.

### `effigy.run-stop.v1`

```json
{
  "schema": "effigy.run-stop.v1",
  "schema_version": 1,
  "run_id": "<run-id>",
  "accepted": true,
  "request_source": "operator",
  "signal": "SIGTERM",
  "grace_ms": 5000,
  "outcome": "cancelled",
  "generation": {
    "boot_id": "<boot-id-or-null>",
    "owner_pid": 1234,
    "owner_start_identity": "<identity-or-null>"
  },
  "confirmed_gone": true,
  "groups": [
    { "pgid": 1240, "state": "gone" }
  ]
}
```

Outcomes are `cancelled`, `waiter_cancelled`, `already_terminal`,
`not_found`, `refused_foreign_checkout`, `refused_untrusted_journal`,
`refused_unauthorized`, and `unavailable_supervisor`. `confirmed_gone` is
`true`, `false`, or `null`; `null` means unproven, never "gone".

### `effigy.run-logs.v1`

```json
{
  "schema": "effigy.run-logs.v1",
  "schema_version": 1,
  "run_id": "<run-id>",
  "state": "running",
  "members": [
    { "member_id": "<id-or-null>", "selector": "<selector>", "log_ref": "<ref>" }
  ],
  "lines": ["<redacted-line>"],
  "truncated": false,
  "complete": true
}
```

`complete = false` when the log could not be captured fully; a partial log
never implies a complete run.

### `effigy.run-capabilities.v1`

```json
{
  "schema": "effigy.run-capabilities.v1",
  "schema_version": 1,
  "platform": "macos",
  "supervision": { "inline": true, "detached": true },
  "stop": true,
  "hard_timeout": true,
  "logs": true,
  "journal_schema": "effigy.run-journal.v2"
}
```

`stop` and `hard_timeout` are false when the platform or journal cannot
provide closure proof. A false capability rejects a dependent declaration
before execution.

## Composition with admission and QA groups

- Heavy runs keep one contract
  [049](049-heavy-validation-admission-contract.md) lease, acquire it before
  setup, and release it through the ordered stop sequence. Non-heavy runs get
  an `evidence_only` record with no lease and no machine-wide resource claim.
- A QA-group run is one run with one generation; its members execute serially
  inside it and keep contract
  [051](051-bounded-qa-groups-contract.md) member states. Stopping the group
  run stops the active member generation and marks later members
  `not_started`; it never synthesizes a pass.
- Group planning rejects an unsupported `hard_timeout_ms` or an
  unsupervisable member route before execution.
- A capacity waiter can be cancelled without stopping or affecting any lease
  owner.

## Migration and compatibility

| Surface | Decision |
| --- | --- |
| Selector status (017/018) | Unchanged. Run control is run-id scoped and separate. |
| `effigy admission` schemas | Unchanged. The unified run status composes the existing admission record; it does not reshape admission JSON. |
| `effigy qa` / CI / release routes | Unchanged. Supervision adds no gate. |
| Managed-session `status`/`logs`/`stop` | Unchanged and still own their managed session. They do not become generic run control. |
| Draft tasks (046) | No new draft grammar. A draft route is supervised like any other once it resolves; admission classification stays governed by 046. |
| Admission store | Additive schema upgrade only. Existing capacity records map to `kind = capacity_lease`; scheduling and reclamation predicates for capacity holders are unchanged. Evidence-only records are invisible to scheduling. |
| JSON consumers | New `effigy.run-*` schemas; existing schema ids are not reshaped. |

The store upgrade and any journal field that influences capacity are
behavioral changes to the host-wide coordinator and require an implementation
brief that proves the unchanged scheduling and reclamation predicates. This
design task ships prose only and marks every surface unavailable.

## Required implementation proof

Private, macOS and Linux, bounded cases. No case may kill a foreign process or
declare an uncertain child gone:

- two checkouts run concurrently; stop one run from its own checkout and prove
  the sibling run, its lease, and its children survive untouched;
- a stop from a different checkout is refused `refused_foreign_checkout` with
  no signal delivered;
- nested children and grandchildren are signalled and closed within the
  recorded generation; an unprovable process is left `unknown`;
- a reused owner PID and a reused group-leader PID with different start
  identities are never signalled and never declared gone;
- a stale journal with a changed boot id or missing owner reconciles to a
  recorded classification, and an uncertain holder keeps capacity reserved;
- an untrusted or unavailable journal fails closed for stop;
- owner `SIGKILL` with a detached witness records `owner_lost` with per-child
  evidence; owner `SIGKILL` inline records `unknown` with no receipt;
- an unavailable supervisor returns `unavailable_supervisor` and claims
  nothing;
- a capacity waiter stop removes only the waiter and leaves every lease owner
  running;
- a declared hard timeout on an unsupported platform or route fails before
  execution with `unsupported_run_control`;
- child signal, owner `SIGTERM`, owner `SIGKILL`, and no-witness interruption
  are distinguished exactly as the evidence table requires.

The group runtime's acceptance case from contract
[051](051-bounded-qa-groups-contract.md) reuses these cases rather than
inventing a second supervision path.

## Implementation cuts

Independently reviewable cuts, each with its own approval:

1. **Run journal and inline supervision for every run.** General run record,
   generation identity, group registration for heavy and non-heavy runs, the
   ordered stop sequence, waiter cancellation, authorization, capability
   probe, and `status`/`logs`/`stop`/`capabilities` JSON. Proof: two-checkout
   sibling survival, foreign-checkout refusal, nested children, PID reuse,
   stale journal, permission failure, waiter cancellation, and unchanged
   admission scheduling/reclamation.
2. **Detached witness, owner-loss evidence, and hard timeout.** Detached
   placement, owner `SIGKILL` attribution, `unavailable_supervisor`, exact
   interruption taxonomy, hard-timeout enforcement, and group-plan capability
   rejection. Proof: owner loss with and without a witness, unsupported-route
   rejection, uncertain-child `unknown`.
3. **Journal upgrade and group integration.** Additive store upgrade with the
   unchanged scheduling/reclamation proof, group-run generation and member
   ledger reuse, and the flip of contract
   [051](051-bounded-qa-groups-contract.md), architecture
   [031](../architecture/031-bounded-qa-groups-runtime.md), and the workflow
   guide from proposed to available.

## Change triggers

Revisit this contract before adding a resident supervisor or service, an
interactive signal API, cross-checkout or cross-user control, changed lease
reclamation, a second execution runtime, window/POSIX-spawn variants, or any
claim of signal attribution without the evidence table.
