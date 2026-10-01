# Host-wide validation admission

Heavy Effigy tasks share one capacity budget per configured state directory.
The built-in selectors `qa`, `ci`, and `ci:fresh` are heavy by default. A repo
can also mark a task table:

```toml
[tasks."qa:ci:fast"]
admission = "heavy"
run = ["cargo test --workspace"]
```

Shorthand tasks keep their existing behavior. Unknown `admission` values fail
manifest parsing. `--plan`, task discovery, and admission queries do not
acquire a lease.

## Binary and invocation coverage

Every participating checkout must invoke an admission-capable Effigy binary.
Probe the binary workers actually resolve, from the consumer checkout:

```sh
command -v effigy
effigy admission status --json
```

The query must return the `effigy.admission.status.v1` result schema. An
undefined `admission` task means the resolved binary predates the command;
a version string alone does not prove capability. Effigy maintainers can
refresh their existing local channel with the
[local installation guide](010-path-installation-and-release.md).

Admission covers Effigy task invocations. Direct `cargo`, `nextest`, or `vitest`
commands do not acquire an Effigy lease. A heavy proof or test selector invoked
outside `qa` needs its own `admission = "heavy"` task table. All participating
invocations must use the same coordinator directory, as described below.

## Capacity and waiting

Effigy measures logical CPU count and physical memory. The default reservation
budget leaves at least half of each available for interactive work. An
unconfigured heavy task reserves the whole budget, so only one such task runs
at a time. Explicit per-invocation reservations use `EFFIGY_ADMISSION_CPU_UNITS`
and `EFFIGY_ADMISSION_MEMORY_MIB`; budget overrides use
`EFFIGY_ADMISSION_CPU_BUDGET` and `EFFIGY_ADMISSION_MEMORY_BUDGET_MIB`.

Reservations limit concurrent admission. They do not enforce OS CPU or memory
limits on arbitrary child processes. Effigy reports measured CPU and peak RSS
where available and marks reservation overruns. Standard task commands receive
the reserved CPU units as `CARGO_BUILD_JOBS`.

Waiters are FIFO within their repository and rotate fairly across repository
heads. A waiter has a 30-minute deadline by default; set
`EFFIGY_ADMISSION_TIMEOUT_SECS` to change it. Expiry is a capacity timeout,
separate from task failure and the task's normal execution timeout. While
waiting, stderr reports its position. Query live state from another process:

```sh
effigy admission status --json
effigy admission run <RUN_ID> --json
effigy admission runs --caller queue:<taskId>:<runId> --json
```

Run records include caller, repository, selector, queue wait, wall time, child
CPU user/system time, peak RSS when measurable, budget, reservation, state,
exit classification, and run IDs. Missing measurements are `null`. Caller
identity is an attribution label, not an authorization token.

## State and recovery

The default journal lives in `~/.cache/effigy/admission`, with private
permissions. Set `EFFIGY_ADMISSION_DIR` to an absolute path when several OS
users need a shared coordinator. Shared directories must be trusted, setgid,
group-readable/writable/searchable, and inaccessible to other groups; all
participating users must configure the same path and trusted group.

Effigy tracks the owner generation and task process groups. It reclaims a
stopped owner's lease only after proving its task process groups are gone. An
uncertain or unverifiable owner remains visible as `stale_owner_unknown` and
continues to reserve capacity. Effigy does not kill another caller's process.

Nested Effigy calls inherit a lease only when the persisted lease is active and
the nested process belongs to the owner's process tree or a registered task
process group. Separate top-level invocations still request separate leases.

Repeated runs do not currently join an owner's execution. Joining stays
disabled unless Effigy can prove the complete input identity and capture the
owner's complete log and exit result.

## Opt-in host scheduler backend

The lease store above is the default and is unchanged. Set
`EFFIGY_HOST_SCHEDULER=1` to send heavy work to the Queue/Nucleus host-run
scheduler instead ([contract 049](../knowledge/contracts/049-heavy-validation-admission-contract.md#opt-in-scheduler-backend)).
Unset or `0` keeps the lease. Anything else exits 2 before doing any work.

```sh
EFFIGY_HOST_SCHEDULER=1 effigy qa
```

What changes for a heavy run (`qa`/`ci`/`ci:fresh`, `admission = "heavy"` tasks
and groups with a heavy member):

- Effigy resolves the selector first, then asks the scheduler to run exactly
  this command. Output streams as normal and the exit status and `--json`
  envelope are the child's. Light tasks, `--plan` and discovery never contact
  the scheduler.
- A heavy run with no reachable scheduler exits 75 with `scheduler_unreachable`.
  It does not fall back to the lease. To run anyway, record why:
  `EFFIGY_SCHEDULER_OVERRIDE="<reason>" EFFIGY_HOST_SCHEDULER=1 effigy qa`.
  The reason is journaled for the scheduler before the run starts.
- Inside a scheduler-launched run, nested `effigy` calls reuse the run through
  `HOST_RUN_TOKEN`. A token that does not validate exits 77 and never queues.
  Do not set it by hand.
- Waiting prints `waiting for QA capacity, position N`. `capacity_timeout`
  means the run never launched. Ctrl-C or SIGTERM asks the scheduler to cancel
  and then reports how the run settled; a cancelled or lost run never exits 0.
- A QA group records a `backend` object (scheduler run, epoch, queue wait or
  `null`). A task sees the scheduler's `PATH` and `HOME`.
- `EFFIGY_ADMISSION_TIMEOUT_SECS`, `EFFIGY_ADMISSION_CPU_UNITS` and
  `EFFIGY_ADMISSION_MEMORY_MIB` still set the capacity wait and reservation.
  `EFFIGY_HOST_SCHEDULER_RUN_TIMEOUT_SECS` (default 7200) sets the run deadline.

Not available: a run stop or logs command, group `hard_timeout_ms`, and any
default activation. Maintainers prove the backend with
`effigy test:host-run:integration` against an isolated Queue checkout at merge
`e9e4d12` (see the test file header); it never touches the live endpoint.
