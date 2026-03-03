# 013 - Testing Orchestration

Effigy supports built-in test runner detection when a project does not define an explicit `tasks.test` command.

## Commands

- `effigy test`
- `effigy test --plan`
- `effigy test --verbose-results`
- `effigy test --tui`

## Detection Order

Per target root:

1. `vitest` when package/config/bin markers are present.
2. `cargo nextest run` when `Cargo.toml` exists and `cargo-nextest` is available.
3. `cargo test` when `Cargo.toml` exists and `cargo-nextest` is unavailable.

`effigy test --plan` prints selected runner, command, evidence, fallback chain, and per-target `cargo-env-match` mode.

## Explicit Override

If `tasks.test` exists in the selected catalog, that explicit task always wins.

## Built-in Cargo Env Auto-Apply

When built-in `test` runs cargo suites (detected or configured), Effigy automatically applies manifest `[env]` entries whose keys start with `CARGO_`.

Included sources:
- direct `[env]` entries (for example `CARGO_HOME = "..."`)
- grouped profile arrays under `[env]` (for example `cargo = [{ CARGO_HOME = "..." }, ...]`)
- fallback for missing `CARGO_HOME` / `CARGO_TARGET_DIR`:
  1. process environment
  2. `<target-root>/.env`

Precedence:
- manifest `[env]` wins over process env and dotenv fallback
- process env wins over dotenv fallback

Matching behavior:
- applies to command shapes that resolve to cargo executables (`cargo`, `cargo-nextest`, `/abs/path/cargo`, `/abs/path/cargo-nextest`)
- supports common wrappers and prefixes before cargo (`env`, `exec`, `command`, leading `KEY=value` assignments)
- in default `prefix-aware` mode, does not apply to shell-wrapped commands where cargo is inside a shell string (for example `sh -lc "cargo test --workspace"`)
- does not apply to non-cargo executables

Matcher tuning:
- default mode is `prefix-aware`
- set `[test].cargo_env_match = "executable-only"` to only match direct cargo executables (no wrappers/prefixes)
- set `[test].cargo_env_match = "shell-aware"` to also match shell-wrapped cargo commands (for example `sh -lc 'cargo nextest run --workspace'`)

Value substitution:
- `{project}` and `{repo}` in `[env]` `CARGO_*` values resolve to the executing catalog root for each built-in test target.

Example:

```toml
[env]
CARGO_HOME = "{project}/.effigy/cargo/home"
CARGO_TARGET_DIR = "{project}/.effigy/cargo/target"

[test.suites]
integration = "env RUST_LOG=info cargo nextest run --workspace"
```

## Task Reference Chains

Task-ref chains (`{ task = "..." }`) can target built-ins (including `test`) and include inline args.

Examples:

```toml
[tasks.validate]
run = [{ task = "test vitest" }, "printf validate-ok"]

[tasks.dev]
mode = "tui"

concurrent = [
  { run = "cargo run -p api", start = 1, tab = 2 },
  { task = "test vitest \"user service\"", start = 2, tab = 1 }
]
```

Notes:
- inline args are parsed with shell-style quoting/escaping.
- quote multi-word args inside the task string.
- parsing is tokenization only; shell expansion features (for example globbing, variable expansion, command substitution) are not applied inside `task = "..."`.

## Workspace Fanout

When built-in `test` is used from a workspace root, Effigy fans out across discovered catalog roots and aggregates results.

Concurrency is configured in root `effigy.toml`:

```toml
[test]
max_parallel = 2
```

If unset, Effigy defaults to `3` workers.

Result rendering:
- default is compact per-target status only,
- `--verbose-results` includes runner/root/cargo-env-match/command details per target.

TUI diagnostics:
- set `EFFIGY_TUI_DIAGNOSTICS=1` when running `effigy test --tui` to emit post-run runtime diagnostics and recent trace lines for emulator/debug troubleshooting.

## Related Guides

- [`022-manifest-cookbook.md`](./022-manifest-cookbook.md)
- [`023-troubleshooting-and-failure-recipes.md`](./023-troubleshooting-and-failure-recipes.md)
- [`025-command-reference-matrix.md`](./025-command-reference-matrix.md)

## Next Step

After finalizing test routing, capture expected machine payloads in [`026-json-payload-examples.md`](./026-json-payload-examples.md) for CI consumers.
