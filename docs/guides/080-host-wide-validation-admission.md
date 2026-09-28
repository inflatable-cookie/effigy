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
