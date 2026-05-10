# 025 - Container Command Decomposition

Generation: `g04`

Status: Active
Owner: Platform
Created: 2026-05-10
Depends on:
- [`024-command-reference-completeness-and-flag-consistency.md`](./024-command-reference-completeness-and-flag-consistency.md)

## Goal

Split `container_command/` (6549 lines) into focused submodules by domain. No
user-facing behavior changes.

## Scope

Current structure:
```
src/runner/container_command/
├── mod.rs          (844 lines, massive match dispatcher)
├── data.rs         (data subcommands)
├── gateway_registration/
├── lifecycle.rs    (up/down/status/stats/logs/shell/reset)
├── support.rs      (shared helpers)
```

Target structure:
```
src/runner/container_command/
├── mod.rs              (thin dispatcher, ~200 lines)
├── lifecycle.rs        (up, down, status, stats, logs, shell, reset, eject)
├── data.rs             (list, export, dump, import, seed, pull-production)
├── cache.rs            (list, prune)
├── volume.rs           (list, prune)
├── gateway_registration/
├── support.rs          (shared helpers)
```

### Extraction Rules

- **Lifecycle:** `up`, `down`, `status`, `stats`, `logs`, `shell`, `reset`, `eject`
- **Data:** `data list`, `data export`, `data dump`, `data import`, `data seed`, `data pull-production`
- **Cache:** `cache list`, `cache prune`
- **Volume:** `volume list`, `volume prune`

`mod.rs` retains only the top-level `run_container` match and shared import
block. Each extracted module exports one public function called by `mod.rs`.

### Fallback Pattern Extraction

`container status`, `container down`, and `container cache list` all implement
the same fallback logic: try repo root, then try cwd. Extract a shared
`resolve_container_scope(repo_override, name)` helper.

## Non-Goals

- No user-facing behavior changes
- No CLI surface changes
- No `.github/workflows/` edits
- No release execution

## Why Now

`container_command` is the largest command module by an order of magnitude. It
slows compilation, complicates code review, and makes it hard to locate logic.
The module has grown organically as container features accumulated. Decomposing
it now prevents further bloat.

## Core Decisions

### Module Boundaries

Modules are grouped by operator concept, not internal dependency:
- **Lifecycle** = container runtime operations (start, stop, inspect, interact)
- **Data** = database and volume data operations
- **Cache** = build cache inventory and cleanup
- **Volume** = Docker volume inventory and cleanup

### Public Interface

Each module exports exactly one function:
```rust
pub(crate) fn run_container_lifecycle(...)
pub(crate) fn run_container_data(...)
pub(crate) fn run_container_cache(...)
pub(crate) fn run_container_volume(...)
```

`mod.rs` calls the appropriate function from its match arms.

### Fallback Helper

```rust
fn resolve_container_scope(
    repo_override: Option<PathBuf>,
    name: Option<String>,
) -> Result<ContainerScope, RunnerError>
```

Returns either a repo-scoped or cwd-scoped container context. Used by status,
down, cache list, and volume list.

## Success Criteria

- `container_command/mod.rs` under 300 lines
- Each new module under 600 lines
- All container tests pass without changes
- No user-facing behavior changes
- `cargo clippy` passes

## Suggested Batch Order

1. Extract `cache.rs` (smallest, lowest risk)
2. Extract `volume.rs`
3. Extract `lifecycle.rs` (largest, but well-scoped)
4. Refactor `data.rs` if needed (already separate file)
5. Extract fallback helper
6. Slim `mod.rs` to dispatcher only

## Validation

- All existing container tests pass
- `cargo test` passes
- `cargo clippy` passes
- `git diff --check`

## Next Task

Execute `646` to extract the cache command family into `cache.rs`.
