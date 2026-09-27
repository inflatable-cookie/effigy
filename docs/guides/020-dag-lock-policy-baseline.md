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
- `timeout_ms`: hard timeout for a step (`124` timeout exit).
- `retry`: retry attempts after the first failure.
- `retry_delay_ms`: delay between retry attempts.
- `fail_fast`: default `true`; set `false` to let sibling ready-steps continue in the current DAG level.

## 3) Lock Scopes

Runtime locks are file-based under `.effigy/locks`:
- `task:<selector>` by default, using the full rendered selector (`qa:docs`,
  `validate:activity-routing`, `acme-api/dev`)
- `shared:<name>` when a task opts into a shared lock name
- `profile:<task>/<profile>` (managed `mode = "tui"` runs)

Independent selectors keep independent task locks. Two validation or QA
selectors can run together unless they share an explicit `lock` name.

On lock conflict, Effigy reports:
- scope
- lock path
- holder pid (when available)
- holder start time epoch ms (when available)
- holder heartbeat (when available)
- remediation hint, including `effigy tasks status <selector>` after a wait

Stale locks are auto-reclaimed when the holder PID is no longer alive, the
recorded process is not an Effigy owner, or the heartbeat lease has expired.
A live owner is never stolen.

Callers may wait for a live owner with `--lock-wait-ms <N>` or
`EFFIGY_LOCK_WAIT_MS`. `0` (the default) fails immediately. When the wait
expires, the error names the live owner and the status command to inspect it.
`effigy tasks status <selector>` keeps the still-running owner, including
in-process sequence tasks; retry after that owner releases. Lock-wait JSON (`effigy.lock-wait.v1`) is `error.details`
in `--json` mode, not text stdout.

## 4) Manual Unlock

Use the built-in unlock command:

```sh
effigy tasks unlock task:dev
effigy tasks unlock validate:activity-routing
effigy tasks unlock shared:dev-stack task:dev profile:dev/admin
effigy tasks unlock --all
effigy tasks unlock --all --yes --json
```

A task selector such as `validate:activity-routing` unlocks
`task:validate:activity-routing`. Incomplete typed scopes such as
`profile:foo` fail with that example instead of targeting a different family.

`--json` returns `effigy.unlock.v1`. Broad unlock actions such as `--all`,
`workspace`, `shared:<name>`, or multiple explicit scopes require confirmation
in a real TTY. Use `--yes` for intentional automation.

## Related Guides

- Operator command walkthrough: [`021-quick-start-and-command-cookbook.md`](021-quick-start-and-command-cookbook.md)
- Manifest patterns (including DAG and concurrent profiles): [`022-manifest-cookbook.md`](022-manifest-cookbook.md)
- Failure remediation recipes: [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md)

## Next Step

After introducing DAG policies or lock-scope changes, run lock conflict scenarios and document expected recovery steps in [`023-troubleshooting-and-failure-recipes.md`](023-troubleshooting-and-failure-recipes.md).
