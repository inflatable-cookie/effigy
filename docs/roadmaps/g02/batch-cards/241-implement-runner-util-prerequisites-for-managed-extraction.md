# 241 Implement Runner Util Prerequisites For Managed Extraction

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move the runner-local utility surfaces that `src/runner/managed/**`
secretly depends on into shared crates (or invert the dependency via
a callback) so card `240` can extract managed without dragging
routing-core work into scope.

## Context

The in-flight attempt at `240` surfaced that managed doesn't just
reach into runner for `RunnerError`, `LoadedCatalog`, and
`ManagedTaskPlan` (the three points `238` captured). It also reaches
into ~500 lines of runner-local utilities:

| Runner symbol | Used by managed | LOC | Other runner users |
|---|---|---|---|
| `catalog::select_catalog_and_task` | `references/resolve.rs` | ~250 (routing core) | routing core |
| `env_schema_support::resolve_catalog_env_schema` | `references/resolve.rs`, `run_spec/.../sources.rs` | 94 | `execute/pipeline/standard.rs` |
| `util::parse_task_reference_invocation` | `references/parser.rs` | ~100 | `tasks_probe/resolve.rs` |
| `util::render_passthrough_args` | `references/parser.rs` | small | yes |
| `util::shell_quote` | `scheduler/script.rs`, `run_spec/command.rs` | trivial | yes |
| `util::parse_dotenv_entries` | (via `env_schema_support`) | ~40 | only via env_schema |
| `model::constants::BUILTIN_TASKS` | `references/parser.rs` | 13 entries | yes |

The decide card (`238`) didn't surface these because the `grep
RunnerError | LoadedCatalog | ManagedTaskPlan` sweep only caught the
three big coupling points. Function-level grep through `managed/**`
reveals the fuller set.

`select_catalog_and_task` is the stickiest — it's part of routing
core (a separate queued batch). The other six surfaces can move
wholesale into existing shared crates.

## In Scope

1. **`util/shell.rs` → `effigy-core`** — `shell_quote` and
   `with_local_node_bin_path` are pure utilities. Expose as
   `effigy_core::shell::{shell_quote, with_local_node_bin_path}`.
2. **`util/dotenv.rs` → `effigy-env`** — `parse_dotenv_entries` is
   env-file parsing, belongs with the env crate.
3. **`env_schema_support.rs` → `effigy-env`** — the 94-line
   `resolve_catalog_env_schema` helper is env-schema resolution;
   runner keeps a thin adapter that maps the shared error into
   `RunnerError`.
4. **`util/parsing/reference.rs` → `effigy-tasks`** — the 100-line
   task-reference parsing (`parse_task_reference_invocation`,
   `render_passthrough_args`) fits alongside the existing parsing
   helpers in `effigy-tasks`.
5. **`BUILTIN_TASKS` list** — duplicate the 13-entry constant into
   `effigy-managed`'s future tree (inline in `lib.rs`) during card
   `240`. No card-`241` action needed.
6. **`select_catalog_and_task` → callback contract in managed** —
   introduce a small trait (or function pointer) inside managed that
   the runner's reference-resolution flow fills in. The runner keeps
   the routing implementation. This inverts the dependency so
   managed no longer reaches into routing core.
7. **Runner-side adapters** — `src/runner/util.rs`, `util/parsing.rs`,
   `util/shell.rs`, `util/dotenv.rs`, `env_schema_support.rs` stay in
   the runner as thin adapters that re-export / wrap the shared
   versions with `RunnerError` conversions. All existing runner call
   sites compile unchanged.

## Out Of Scope

- Moving the managed tree itself (that's `240`).
- Extracting routing core (`catalog/**`, `scan/**`, `locking/**`,
  `deferral/**`) — separate queued batch.
- Extracting built-in tasks (`builtin/**`) — separate queued batch.
- Reshaping `RunnerError`.
- Changing the observable behaviour of any of the moved helpers.

## Acceptance Criteria

- `effigy-core` exposes the shell utility surface as
  `effigy_core::shell::*`.
- `effigy-env` exposes `parse_dotenv_entries` and
  `resolve_catalog_env_schema` (the latter with a shared error type
  that converts into `RunnerError` at the runner boundary).
- `effigy-tasks` exposes `parse_task_reference_invocation` and
  `render_passthrough_args`.
- A callback contract for "resolve a task reference against loaded
  catalogs" is defined at whatever layer makes sense (either in
  `effigy-manifest` alongside `LoadedCatalog`, or as a trait that
  managed will own post-`240`). The runner provides the impl.
- Runner-side adapter modules stay in place so existing call sites
  (including inside the yet-to-move `src/runner/managed/**`) compile
  unchanged.
- `cargo test --workspace` green.
- `cargo fmt --all -- --check` and
  `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err
  -A clippy::too_many_arguments -A clippy::type_complexity` clean.

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err
  -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Execute
[`240-implement-effigy-managed-extraction.md`](./240-implement-effigy-managed-extraction.md)
to move `src/runner/managed/**` and `runner::model::managed` into the
new `effigy-managed` crate now that the shared utilities are
reachable from a crate dep graph instead of a runner-internal path.
