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
- `task:<name>` by default
- `shared:<name>` when a task opts into a shared lock name
- `profile:<task>/<profile>` (managed `mode = "tui"` runs)

On lock conflict, Effigy reports:
- scope
- lock path
- holder pid (when available)
- holder start time epoch ms (when available)
- remediation hint

Stale locks are auto-reclaimed when the holder PID is no longer alive.

## 4) Manual Unlock

Use the built-in unlock command:

```sh
effigy unlock task:dev
effigy unlock shared:dev-stack task:dev profile:dev/admin
effigy unlock --all
effigy unlock --all --json
```

`--json` returns `effigy.unlock.v1`.

## Related Guides

- Operator command walkthrough: [`021-quick-start-and-command-cookbook.md`](./021-quick-start-and-command-cookbook.md)
- Manifest patterns (including DAG and concurrent profiles): [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- Failure remediation recipes: [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)

## Next Step

After introducing DAG policies or lock-scope changes, run lock conflict scenarios and document expected recovery steps in [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md).
