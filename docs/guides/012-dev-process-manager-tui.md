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

[shell]
run = "$SHELL"
```

Profile entries support:
- direct task references (`catalog/task`), or
- local process ids defined under `[tasks.dev.processes.<name>]`.

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
- Optional shell tab when `[tasks.dev].shell = true`.
- Active tab receives typed input; `Enter` sends input to that process.
- `Tab` / `Shift+Tab` cycles tabs.
- `q` or `Ctrl+C` exits and terminates child processes.

## 4) Environment Controls

- `EFFIGY_MANAGED_STREAM=1`
  - bypasses TUI and runs selected profile in stream mode.
- `EFFIGY_MANAGED_TUI=0|false`
  - disables TUI auto-launch and renders managed plan output.
- `EFFIGY_MANAGED_TUI=1|true`
  - forces TUI launch.

## 5) Validation Checklist

1. Run `effigy dev` from repo root and verify all default profile tabs open.
2. Run `effigy dev <profile>` and verify only selected processes appear.
3. In a Vite tab, send `r` then `Enter` and confirm restart behavior.
4. Switch to shell tab and run ad-hoc command in same workspace root.
5. Exit with `q` and verify child process teardown.
