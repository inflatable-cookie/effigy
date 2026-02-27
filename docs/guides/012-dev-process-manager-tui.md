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

[tasks.dev.profiles]
default = ["farmyard/api", "farmyard/jobs", "cream/dev", "dairy/dev"]
admin = ["farmyard/api", "farmyard/jobs", "dairy/dev"]

```

Profile entries support:
- direct task references (`catalog/task`), or
- local process ids defined under `[tasks.dev.processes.<name>]`.
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

[tasks.dev.profiles]
default = ["api", "front"]

[tasks.dev.processes.api]
run = "cargo run -p farmyard-api"

[tasks.dev.processes.front]
task = "cream/dev"
```

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
