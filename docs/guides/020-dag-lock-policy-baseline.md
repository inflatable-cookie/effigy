# 020 - DAG Lock and Policy Baseline

This guide covers the compact DAG run schema, step policy controls, and lock behavior introduced for roadmap 010.


## Vision Alignment

- Primary tags: `MAINT`, `OPERATE`
- Target movement: orchestration policy and lock behavior remain deterministic under concurrent workflows.

## 1) DAG Run Steps

Effigy supports linear sequences and DAG-style dependencies in `tasks.<name>.run`.

```toml
[tasks.validate]
run = [
  { id = "tests", task = "test vitest \"user service\"" },
  { id = "report", run = "printf validate-ok", depends_on = ["tests"] }
]
```

Rules:
- `id` must be unique when present.
- `depends_on` values must reference existing step `id`s.
- cycles fail fast with cycle evidence.
- if no `depends_on` values are used, the run remains linear.

## 2) Step Policy

Each run-step table can define node-level policy:

```toml
[tasks.validate]
run = [
  { id = "tests", task = "test vitest \"user service\"", timeout_ms = 120000, retry = 1, retry_delay_ms = 250 },
  { id = "report", run = "printf validate-ok", depends_on = ["tests"], fail_fast = false }
]
```

Policy keys:
- `timeout_ms`: hard timeout for a step (`124` timeout exit). On Unix the step
  runs in a new process group; expiry stops that group and reaps the direct
  child. The caller group is never signalled. Non-Unix hosts kill only the
  direct child pid.
- `retry`: retry attempts after the first failure.
- `retry_delay_ms`: delay between retry attempts.
- `fail_fast`: default `true`; set `false` to let sibling ready-steps continue in the current DAG level.

## 3) Lock Scopes

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

- Operator command walkthrough: [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- Manifest patterns (including DAG and concurrent profiles): [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- Failure remediation recipes: [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)

## Next Step

After introducing DAG policies or lock-scope changes, run lock conflict scenarios and document expected recovery steps in [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md).
