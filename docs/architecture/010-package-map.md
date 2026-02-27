# Package Map

## Rust crate

| Package | Purpose |
|---|---|
| `effigy` | CLI binary + library implementing command parsing, resolution, catalog execution, and built-in tasks |

## Core source modules

| Module | Responsibility |
|---|---|
| `src/lib.rs` | CLI command model, parse/usage contract |
| `src/resolver.rs` | Root resolution and workspace promotion |
| `src/runner/mod.rs` | Runner orchestration and command execution entrypoints |
| `src/runner/manifest.rs` | Manifest schema + serde parsing/normalization |
| `src/runner/model.rs` | Shared runner domain types/constants |
| `src/runner/catalog.rs` | Catalog discovery and selection strategy |
| `src/runner/builtin.rs` | Built-in task dispatch + built-in test orchestration |
| `src/runner/managed.rs` | Managed process plan resolution + execution |
| `src/runner/deferral.rs` | Deferral selection/execution pipeline |
| `src/runner/render.rs` | Runner-specific rendering and trace formatting |
| `src/runner/util.rs` | Shared runner utility helpers (parse/select/shell/path) |
| `src/tui/mod.rs` | Reusable TUI namespace exports |
| `src/tui/multiprocess/mod.rs` | Multi-process TUI orchestration |
| `src/tui/multiprocess/render.rs` | TUI rendering/layout layer |
| `src/tui/multiprocess/terminal_text.rs` | ANSI/vt100 parsing + terminal text shaping |
| `src/tasks/mod.rs` | Task trait contracts and shared task types |
| `src/tasks/pulse.rs` | Built-in `repo-pulse` task implementation |
| `src/bin/effigy.rs` | Binary entrypoint |

## Runtime artifacts

| Artifact | Description |
|---|---|
| `effigy.toml` | Canonical task catalog manifest |
