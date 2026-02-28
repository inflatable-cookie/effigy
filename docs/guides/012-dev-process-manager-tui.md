# 012 - Dev Process Manager TUI

This guide explains how to define and run managed dev process tasks.

## 1) Invocation

Managed task contract:

```bash
effigy dev
effigy dev <profile>
```

- `effigy dev` resolves `profiles.default`.
- `effigy dev <profile>` resolves that profile name.
- On interactive terminals, Effigy launches the ratatui manager.
- On non-interactive terminals, Effigy renders a managed plan summary.

## 2) Manifest Shape

Recommended compact profile schema:

```toml
[tasks.dev]
mode = "tui"
shell = true

concurrent = [
  { task = "catalog-a/api", start = 1, tab = 3 },
  { task = "catalog-a/jobs", start = 2, tab = 4, start_after_ms = 1200 },
  { task = "catalog-b/dev", start = 3, tab = 2 },
  { run = "my-other-arbitrary-process", start = 4, tab = 1 }
]

[tasks.dev.profiles.admin]
concurrent = [
  { task = "catalog-a/api", start = 1, tab = 2 },
  { task = "catalog-a/jobs", start = 2, tab = 3, start_after_ms = 1200 },
  { task = "catalog-c/dev", start = 3, tab = 1 }
]
```

Profile entries support:
- direct task references (`catalog/task`) via `task = "..."`, or
- arbitrary process commands via `run = "..."`, or
- relative path task references (`../repo/task`) via `task = "..."`, resolved from the current catalog root.
- optional profile overrides via `[tasks.dev.profiles.<name>]` with their own `concurrent = [...]`.
- optional integrated shell tab when `shell = true`.

Optional global shell command override:

```toml
[shell]
run = "exec ${SHELL:-/bin/zsh} -i"
```

If omitted, Effigy uses:
- `exec ${SHELL:-/bin/zsh} -i`

Example mixed mode:

```toml
[tasks.dev]
mode = "tui"

concurrent = [
  { run = "cargo run -p app-api", start = 1, tab = 2 },
  { task = "catalog-b/dev", start = 2, tab = 1 }
]
```

Example relative repo reference:

```toml
[catalog]
alias = "dairy"

[tasks.dev]
mode = "tui"

concurrent = [
  { task = "../shared/validate", start = 1, tab = 1 }
]
```

In this example, `../shared/validate` resolves relative to `dairy` catalog root.

## 3) Runtime Behavior

- One tab per managed process.
- When `shell = true`, includes an additional `shell` tab.
- Non-shell tabs use input panel mode (`Tab` toggles command/insert; `Enter` sends input).
- Shell tab uses direct terminal capture mode:
  - `Ctrl+G` toggles shell capture on/off.
  - when capture is on, keypresses go directly to shell (including `Tab` completion).
  - shell tab label shows `shell [live]` when capture is active.
- `Tab` / `Shift+Tab` cycles tabs.
- `q` or `Ctrl+C` exits and terminates child processes.

## 4) Environment Controls

- `EFFIGY_MANAGED_STREAM=1`
  - bypasses TUI and runs selected profile in stream mode.
- `EFFIGY_MANAGED_TUI=0|false`
  - disables TUI auto-launch and renders managed plan output.
- `EFFIGY_MANAGED_TUI=1|true`
  - forces TUI launch.
- `EFFIGY_TUI_DIAGNOSTICS=1|true`
  - enables post-run TUI diagnostics summary (event/key/frame counters and recent trace lines) for debugging emulator/runtime behavior.

## 5) Validation Checklist

1. Run `effigy dev` from repo root and verify all default profile tabs open.
2. Run `effigy dev <profile>` and verify only selected processes appear.
3. In a Vite tab, send `r` then `Enter` and confirm restart behavior.
4. Use another terminal tab/window for ad-hoc commands while the dev stack is running.
5. Exit with `q` and verify child process teardown.
