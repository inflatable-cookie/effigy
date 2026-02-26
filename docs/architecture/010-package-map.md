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
| `src/runner.rs` | Catalog discovery, task selection, command execution |
| `src/tasks/mod.rs` | Task trait contracts and shared task types |
| `src/tasks/pulse.rs` | Built-in `repo-pulse` task implementation |
| `src/bin/effigy.rs` | Binary entrypoint |

## Runtime artifacts

| Artifact | Description |
|---|---|
| `effigy.tasks.toml` | Canonical task catalog manifest |
| `underlay.tasks.toml` | Legacy manifest fallback (catalog-local precedence to canonical file) |
