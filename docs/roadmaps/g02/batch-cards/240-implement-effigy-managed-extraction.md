# 240 Implement Effigy Managed Extraction

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Move `src/runner/managed/**` (37 files, ~4.1k lines) and the two
`runner::model::managed` shapes into a new `effigy-managed` crate.
Replace managed's use of `crate::runner::error::RunnerError` with a
managed-local `ManagedError`, wired to `RunnerError` via a runner-side
`From` impl following the `effigy-process` / `effigy-ui` pattern.

This card implements decisions `1`, `2`, `4` (phase 2), and `5` from
`238`. It assumes `239` has landed (`LoadedCatalog` now lives in
`effigy-manifest`) and `241` has landed (runner utilities now live
in shared crates, and reference resolution is callback-driven).

An in-flight attempt on `240` (pre-revert) surfaced that managed
also depends on ~500 lines of runner-local utilities
(`catalog::select_catalog_and_task`, `env_schema_support`,
`util::shell_quote`, `util::parse_task_reference_invocation`,
`util::parse_dotenv_entries`, `model::constants::BUILTIN_TASKS`,
etc.). Those prerequisites became card `241` rather than being
folded into `240`.

## In Scope

- Create `crates/effigy-managed` with a Cargo.toml depending on
  `effigy-manifest`, `effigy-process`, `effigy-tasks`, `effigy-tui`,
  `effigy-ui`, and `effigy-core`.
- Move these source trees into the new crate:
  - `src/runner/managed/command.rs`
  - `src/runner/managed/plan.rs` + `plan/**`
  - `src/runner/managed/presentation.rs`
  - `src/runner/managed/profiles.rs`
  - `src/runner/managed/references.rs` + `references/**`
  - `src/runner/managed/render_support.rs`
  - `src/runner/managed/run_spec.rs` + `run_spec/**`
  - `src/runner/managed/runtime.rs` + `runtime/**`
  - `src/runner/managed/scheduler.rs` + `scheduler/**`
- Move `src/runner/model/managed.rs` (`ManagedProcessSpec`,
  `ManagedTaskPlan`) into `effigy-managed` public API.
- Introduce `effigy_managed::ManagedError` with variants for every
  `RunnerError` case the managed tree currently produces. Add
  `From<ManagedError> for RunnerError` to `src/runner/error.rs`.
- Rewrite managed imports:
  - `crate::runner::error::RunnerError` → `crate::ManagedError`
  - `crate::runner::manifest::*` → `effigy_manifest::*`
  - `crate::runner::model::catalog::*` → `effigy_manifest::*` (post-`239`)
  - `crate::runner::model::managed::*` → `crate::*` (now internal)
  - `crate::tui::*` → `effigy_tui::*`
  - `effigy_process::*`, `effigy_ui::*` → unchanged
- Leave `src/runner/managed.rs` as a thin shim:
  `pub(in crate::runner) use effigy_managed::*;`
- Leave `src/runner/model/managed.rs` as a thin shim if any caller
  outside managed still reaches it; otherwise delete.
- Preserve public API surface for the three external consumers
  unchanged:
  - `src/runner/demo_command.rs`
  - `src/runner/execute/pipeline/{command,managed}.rs`
  - `src/runner/builtin/test/{execution,planning/...}`

## Out Of Scope

- Rewiring the three external consumers to hit `effigy-managed`
  directly (follow-up sweep after both shims are stable).
- Any change to `RunnerError` beyond adding the `From<ManagedError>`
  impl.
- Extracting routing core (`catalog/`, `scan/`, `locking/`, `deferral/`)
  or built-in tasks (`builtin/`) — separate queued batches.
- Touching any parallel-thread-owned file unless it's a pure import
  rewrite for `LoadedCatalog` or `ManagedTaskPlan`.

## Acceptance Criteria

- `crates/effigy-managed` exists with the moved code.
- `src/runner/managed.rs` is a thin shim (no type definitions, no
  non-trivial logic).
- `src/runner/model/managed.rs` is either a thin shim or deleted.
- The three external consumers compile unchanged.
- `ManagedError` owns every error case the managed tree used to raise
  as `RunnerError`, with a runner-side `From` impl.
- `cargo test --workspace` green.
- `cargo fmt --all -- --check` and
  `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err
  -A clippy::too_many_arguments -A clippy::type_complexity` clean.
- Root-crate `/src/runner/managed/` footprint drops from ~4.1k lines to
  ~10 lines of re-exports.

## Validation

- `cargo test --workspace`
- `cargo test -p effigy-managed`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err
  -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open a post-extraction decide card analogous to `233` / `236` / `237`:
re-survey the remaining `src/runner/**` footprint, confirm the managed
adapter residue is clean, and pick between
- opening the next queued batch (built-in tasks, ~9.5k lines), or
- opening the task-routing-core batch (~6k lines), or
- pausing the lane on an honest boundary.
