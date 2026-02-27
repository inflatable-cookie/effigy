# 013 - Testing Orchestration

Effigy supports built-in test runner detection when a project does not define an explicit `tasks.test` command.

## Commands

- `effigy test`
- `effigy test --plan`
- `effigy test --verbose-results`

## Detection Order

Per target root:

1. `vitest` when package/config/bin markers are present.
2. `cargo nextest run` when `Cargo.toml` exists and `cargo-nextest` is available.
3. `cargo test` when `Cargo.toml` exists and `cargo-nextest` is unavailable.

`effigy test --plan` prints selected runner, command, evidence, and fallback chain.

## Explicit Override

If `tasks.test` exists in the selected catalog, that explicit task always wins:

```toml
[tasks.test]
run = "bun test {args}"
```

## Workspace Fanout

When built-in `test` is used from a workspace root, Effigy fans out across discovered catalog roots and aggregates results.

Concurrency is configured in root `effigy.toml`:

```toml
[builtin.test]
max_parallel = 2
```

If unset, Effigy defaults to `3` workers.

Result rendering:
- default is compact per-target status only,
- `--verbose-results` includes runner/root/command details per target.
