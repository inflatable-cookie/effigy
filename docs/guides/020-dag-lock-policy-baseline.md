# 020 - Ordered Sequences and Task Locks

Run arrays execute in declaration order. A failed, timed-out or stopped step
prevents later steps from starting. The same engine runs host sequences in text
and JSON modes; JSON only captures output and reports each step.

```toml
[tasks.validate]
run = [
  { id = "tests", task = "test vitest \"user service\"" },
  { id = "report", run = "printf validate-ok", timeout_ms = 120000 }
]
```

Each step is a child kernel run. Host shell, built-in and Rhai processes run
under custody; nested task references acquire their canonical Task gate.
References to an active ancestor fail with a cycle error before gate acquisition.
A nested task inherits a shared lock already held by an ancestor; unrelated
holders still exclude it. Nested admissions fail immediately on other busy gates,
so opposing reference trees cannot wait on one another.

`id` labels a step and must be non-empty and unique when supplied. `timeout_ms`
bounds host steps, including nested task commands. Expiry returns 124 and settles
owned descendants before any later step can start. INT, TERM and HUP stop the
active step and return 128 + signal. Owner SIGKILL triggers sentinel settlement.
Rerunning starts at step one after settling an interrupted predecessor; it never
resumes mutation steps silently.

Sequence `depends_on`, `retry`, `retry_delay_ms` and `fail_fast` were removed in
v0.15. Remove these keys and place steps in their required order. Use the invoked
tool's retry policy when needed. Container steps run in the same engine: the
workload runs in its service container under the step's remote custody, and a
stop, signal or timeout reaches the container session, not only the local
client. Guest re-entry remains unchanged. Container setup embedded in a
lifecycle-managed process command retains its existing readiness and startup
order and reports only its process exit until the run supervisor takes custody
of managed processes.

## Lock Scopes

Every task run holds a kernel gate for its resolved task identity
([017](../knowledge/contracts/017-task-status-record-and-active-run-model-contract.md)):

- `task:<selector>`: one gate per declared task. `build` from inside a catalog,
  `api/build` and `./apps/api/build` are the same task and share it; `a:b` and
  `a-b` never do.
- `shared:<name>` when a task opts into a shared lock name; every task
  declaring the name excludes the others.
- A managed profile is recorded on its run; the gate stays per task.

Independent tasks keep independent gates. Two validation or QA selectors can
run together unless they share an explicit `lock` name.

The gate is held exactly while the run's Effigy process lives and is released
when it exits, `kill -9` included. Nothing reclaims, steals or unlocks a live
gate, and a SIGSTOPped holder keeps it.

On conflict (exit 75), Effigy names the holder from the gate probe: pid on this
host, run id, age, and `stopped` when SIGSTOPped, plus
`effigy tasks status <selector>` and the wait flag. JSON callers get
`effigy.lock-wait.v2` as `error.details`.

Callers may wait for a live run with `--lock-wait-ms <N>` before the task name,
or `EFFIGY_LOCK_WAIT_MS`. `0` (the default) fails immediately.

## Related Guides

- [Manifest cookbook](022-manifest-cookbook.md)
- [Task routing](016-task-routing-precedence.md)
- [JSON output](017-json-output-contracts.md)

## Next Step

Use `effigy --plan <task>` to inspect selection, then run the targeted task and
inspect `effigy tasks status <task>` for child-run outcomes.
