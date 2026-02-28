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
| `src/runner/execute.rs` | Manifest task execution helpers and task run rendering snippets |
| `src/runner/builtin/mod.rs` | Built-in task dispatch |
| `src/runner/builtin/help.rs` | Built-in `help` rendering |
| `src/runner/builtin/doctor.rs` | Built-in `doctor` dispatch |
| `src/runner/builtin/tasks.rs` | Built-in `tasks` dispatch + argument parsing |
| `src/runner/builtin/test.rs` | Built-in `test` detection/planning/fanout execution |
| `src/runner/doctor.rs` | Doctor health checks, remediation findings, and report rendering |
| `src/runner/managed.rs` | Managed process plan resolution + execution |
| `src/runner/deferral.rs` | Deferral selection/execution pipeline |
| `src/runner/render.rs` | Runner-specific rendering and trace formatting |
| `src/runner/util.rs` | Shared runner utility helpers (parse/select/shell/path) |
| `src/tui/mod.rs` | Reusable TUI namespace exports |
| `src/tui/core.rs` | Shared TUI primitives (input/log state models and key-navigation helpers) |
| `src/tui/multiprocess/mod.rs` | Multi-process TUI orchestration |
| `src/tui/multiprocess/config.rs` | Multiprocess runtime tuning constants (buffers, tick cadence, vt dimensions) |
| `src/tui/multiprocess/state.rs` | Shared multi-process TUI runtime state and domain enums |
| `src/tui/multiprocess/events.rs` | Process stream ingestion + keyboard interaction handling |
| `src/tui/multiprocess/lifecycle.rs` | Terminal setup/teardown and post-run summary rendering |
| `src/tui/multiprocess/view_model.rs` | Active-tab render data derivation (scroll/cursor/meta) |
| `src/tui/multiprocess/render.rs` | TUI render orchestration and layout routing |
| `src/tui/multiprocess/render/header.rs` | Header/tab chrome rendering |
| `src/tui/multiprocess/render/panes.rs` | Output/input pane rendering + shell caret/scrollbar behavior |
| `src/tui/multiprocess/render/footer.rs` | Footer/status row rendering |
| `src/tui/multiprocess/render/help_overlay.rs` | Help + options overlay rendering and options metadata |
| `src/tui/multiprocess/terminal_text.rs` | ANSI/vt100 parsing + terminal text shaping |
| `src/tasks/mod.rs` | Shared task types (currently root-resolution mode enums) |
| `src/bin/effigy.rs` | Binary entrypoint |

## Runtime artifacts

| Artifact | Description |
|---|---|
| `effigy.toml` | Canonical task catalog manifest |
