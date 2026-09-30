# 032 - Owned Run Supervision Runtime

Contract: [052](../contracts/052-owned-run-supervision-contract.md)
Status: proposed and unavailable until implementation.

## Placement

Run supervision sits under the canonical execution pipeline and above the
host-wide coordinator. It generalizes the signal forwarding, process-group
registration, and crash recovery that contract
[049](../contracts/049-heavy-validation-admission-contract.md) already
implements for heavy admission so that every run, every nested child, and
every QA-group member shares one boundary.

```text
canonical TaskExecutionRequest (contract 013)
                 |
                 v
          run supervisor boundary
   run id + generation + placement
   group registration + forwarding
                 |
                 v
      cancellation / timeout / stop
                 |
                 v
       host-wide run journal (contract 049 coordinator)
       lock + atomic writes + boot identity
```

The boundary resolves a run id, not a selector or a PID. QA-group runs
(contract [051](../contracts/051-bounded-qa-groups-contract.md)) and managed
sessions call the same resolver; neither gets a private cancellation path.

## Current lifecycle facts

The design reuses what exists today and names the gap.

- `crates/effigy-process` spawns each child in its own process group
  (`setpgid`) and signals a tree with an `ppid` walk. It has no run identity
  and no ownership proof.
- `src/runner/admission.rs` records the heavy owner PID, owner start identity,
  boot identity, and supervised process groups in a host-wide locked store.
  It forwards `SIGINT`/`SIGTERM`/`SIGHUP` to registered groups through
  `LeaseScope` and recovers stale owners fail-closed.
- `src/runner/execute/entry.rs` enters `LeaseScope` only after a heavy lease
  is acquired. `src/runner/execute/process_run.rs` calls
  `register_process_group` for every task, but the record is dropped when no
  running capacity lease exists, so non-heavy runs register nothing.
- `crates/effigy-managed` runs a supervisor process for a headless managed
  session, records `session.json`, and stops by pid plus descendant proof.
  That supervisor is the owner process itself.
- `src/runner/host_process.rs` already runs a detached, `setsid`, per-entry
  supervisor with a PID file and signal/escalation handling for container
  host processes. It is the existing shape for a surviving witness.

The gap is exactly one boundary: a run record for every invocation, a
generation a stop can prove, and interruption evidence that survives the
owner.

## Run journal

The journal is the contract
[049](../contracts/049-heavy-validation-admission-contract.md) coordinator
store extended additively:

- one record per run, at creation, before lease acquisition or child spawn;
- `kind = capacity_lease` records participate in capacity scheduling;
  `kind = evidence_only` records do not and are ignored by the scheduler and
  by capacity accounting;
- the existing capacity-holder predicates, reclamation rules, and budget math
  are unchanged for `capacity_lease` records;
- group registration appends each child group with its leader start identity;
- terminal writes set state, classification, interruption evidence, timing,
  and log reference.

The store keeps its lock, temp+rename write, fsync, ownership, permission, and
symlink validation from contract
[049](../contracts/049-heavy-validation-admission-contract.md). A schema
version bump maps existing records to `capacity_lease` with no behavior
change.

## Supervisor placements

### Inline

The owner process installs the signal forwarder and group observer at run
start and holds them for the run. Heavy runs keep their lease acquisition and
use the same scope with the lease id as the run id. Non-heavy runs create an
`evidence_only` record and register groups against it. The owner writes the
terminal record and releases only its own lease.

`effigy-process` keeps spawning children; the boundary registers them. No
second execution engine appears.

### Detached witness

A run that declares `detached` starts one per-run witness before side effects.
The witness:

- creates its own session so the owner's `SIGKILL` cannot reach it, and reads
  the run journal independently;
- mirrors group registration and stop/timeout actions into the journal before
  acting, so evidence survives owner loss;
- proves the owner generation gone by start identity, records `owner_lost`,
  and reports each child `gone`/`live`/`unknown` from the recorded generation;
- starts no task work and owns no task semantics. The canonical owner process
  stays the only executor; the witness never spawns, executes, or reinterprets
  a task.

Closure is proven only from recorded groups; an unregistered or reparented
descendant is `unknown`. The witness reuses the detached-supervisor shape
from `src/runner/host_process.rs` (new session, identity file, signal
forwarding, escalation) rather than adding a daemon. Its lifetime is one run
generation. It has no listening socket and no cross-run authority.

If closure cannot be proven from the placement, the boundary records
`unavailable_supervisor` and claims nothing; the plan/capability gate rejects
a dependent `hard_timeout_ms` before execution.

## Cancellation flow

`effigy tasks run stop` resolves the record, checks authorization, persists
the request, then signals proven generations in the ordered sequence of
contract [052](../contracts/052-owned-run-supervision-contract.md). Waiters
cancel without a signal. The owner or witness writes the terminal record; if
neither survives, reconciliation writes `unknown`.

`hard_timeout_ms` is enforced by the same flow, starts after admission, and
records `timed_out`. It is available only when the capability probe proves
closure for the resolved route.

## Evidence and reconciliation

The journal records:

- `signal_delivered` only when the writing process called `kill()` and saw
  the result;
- `child_signal_observed` only from a surviving reaper or an observed signal
  termination;
- `owner_received_signal` only from the owner's handler;
- a per-group `live`/`gone`/`unknown` state from boot id, start identity, and
  group probe.

No killer identity is invented. Reconciliation uses the same generation
evidence: proven gone is terminal, uncertain is `unknown`, and capacity stays
reserved for an uncertain holder. Contract
[049](../contracts/049-heavy-validation-admission-contract.md) reclamation
rules are unchanged.

## Boundaries

- Keep selector status (contracts 017/018) and the
  [051](../contracts/051-bounded-qa-groups-contract.md) group ledger separate
  from run control; a run id is not a selector.
- Keep managed-session controls as the managed-session surface; do not
  replace them with generic run control.
- Keep the contract
  [013](../contracts/013-task-execution-request-contract.md) pipeline the
  only execution path.
- Do not claim OS resource enforcement, a hard memory/CPU cap, or a result
  join from supervision records.
- Do not change admission scheduling or lease reclamation without a separate
  approved brief and proof.
- Use the existing detached-supervisor mechanism; do not add a resident
  service, agent, or daemon.

## Implementation proof

The runtime must prove definition and generation identity, inline and detached
placement, group registration for heavy and non-heavy runs, the ordered stop
sequence, waiter cancellation, capability-gated hard timeout, the exact
interruption taxonomy, unchanged admission scheduling/reclamation, and every
case listed in contract
[052](../contracts/052-owned-run-supervision-contract.md). The QA-group
acceptance case reuses the same cases.
