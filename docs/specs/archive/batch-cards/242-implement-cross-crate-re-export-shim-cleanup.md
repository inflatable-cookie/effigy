# 242 Implement Cross-Crate Re-Export Shim Cleanup

Status: complete
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Remove the cross-crate re-export shims that the recent extraction batches
(`239`, `240`, `246`) left in place as transitional compatibility layers.
Migrate their call sites to the real crate paths so every workspace type
has exactly one canonical import path.

Internal `pub use self::submod::*` flattening inside each crate's `lib.rs`
is **not** in scope — that is normal Rust public-API surfacing, not a shim.

## Context

Three in-flight migration shims and one cross-crate facade survived the
managed extraction:

| Shim | Re-exports | Call sites |
|---|---|---|
| `src/runner/managed.rs` | `effigy_managed::{command, plan, presentation, profiles, run_spec}` | 5 files |
| `src/runner/model/managed.rs` | `effigy_managed::{ManagedProcessSpec, ManagedTaskPlan}` | 1 file |
| `src/runner/model/catalog.rs` | `effigy_manifest::{DeferredCommand, LoadedCatalog, TaskSelection}` + `effigy_tasks::{CatalogSelectionMode, TaskRuntimeArgs, TaskSelector}` | 24 files |
| `crates/effigy-ui/src/lib.rs` widget re-exports | `effigy_core::widgets::{KeyValue, MessageBlock, NoticeLevel, StepState, SummaryCounts, TableSpec}` | ~20 files |

These shims were justified as transitional cover for the batches that
created them (`238`'s decision 5, and the ergonomic facade inherited from
the original UI extraction at `235`). They are no longer pulling weight:

- Every call site has the real crate already in its dep graph (directly
  or transitively) — migration is a mechanical import rewrite.
- Two canonical paths per type (`crate::runner::model::catalog::LoadedCatalog`
  and `effigy_manifest::LoadedCatalog`, or `effigy_ui::KeyValue` and
  `effigy_core::widgets::KeyValue`) harms grep-ability and hides where
  types actually live.
- The shim files carry no semantics — they are pure `pub use` lines.

## In Scope

1. **Delete `src/runner/managed.rs`** and migrate 5 call sites to
   `effigy_managed::{command, run_spec}` directly.
2. **Delete `src/runner/model/managed.rs`** and migrate the 1 remaining
   inline call site (`src/runner/demo_command.rs:1635`) to
   `effigy_managed::ManagedTaskPlan`.
3. **Delete `src/runner/model/catalog.rs`** and migrate 24 call sites,
   splitting each import by real source crate:
   - `LoadedCatalog`, `TaskSelection`, `DeferredCommand` → `effigy_manifest::`
   - `CatalogSelectionMode`, `TaskRuntimeArgs`, `TaskSelector` → `effigy_tasks::`
4. **Remove widget re-exports from `crates/effigy-ui/src/lib.rs`**
   (the `pub use effigy_core::widgets::{...}` block, lines 18–22). Migrate
   ~20 caller files across `src/`, `crates/effigy-managed`,
   `crates/effigy-tui`, and `crates/effigy-cli` by splitting each mixed
   `use effigy_ui::{...}` statement into two lines: one for `effigy_ui`
   native types (`Renderer`, `PlainRenderer`, `OutputMode`, `UiError`,
   `UiResult`, `SpinnerHandle`), one for `effigy_core::widgets::*`.
5. **Add `effigy-core` to Cargo manifests** of any crate that gained a
   direct `effigy_core::widgets` import but did not previously depend on
   `effigy-core`. Expected: `effigy-managed`, `effigy-tui` (if not already
   present through transitive deps that `cargo` won't accept).
6. **Drop `src/runner/model.rs`** if the module becomes empty after the
   `catalog.rs` and `managed.rs` submodules are removed. If other
   submodules remain, trim accordingly.

## Out Of Scope

- Internal `pub use self::submod::*` flattening inside any crate's
  `lib.rs` — that's the crate's public API, not a shim, and removing it
  would force worse paths like `effigy_ui::renderer::Renderer` on callers.
- Error-type re-exports (e.g., `From<ManagedError> for RunnerError` impls)
  — those are not shims, they are boundary adapters.
- Widening or narrowing any type's visibility beyond what the migration
  mechanically requires.
- Any routing-core or built-in-tasks work (queued batches, see cards
  `243` and `244`).
- Documentation churn beyond updating the 010 lane doc checkpoint.

## Acceptance Criteria

- The three shim files (`src/runner/managed.rs`, `src/runner/model/managed.rs`,
  `src/runner/model/catalog.rs`) are deleted.
- `crates/effigy-ui/src/lib.rs` no longer contains `pub use effigy_core::*`.
- No workspace `lib.rs` or `mod.rs` contains a `pub use <other_crate>::`
  re-export. (Internal `pub use self::...` / `pub use crate::...` stays.)
- All call sites import from the real source crate (`effigy_manifest`,
  `effigy_managed`, `effigy_tasks`, `effigy_core::widgets`).
- `cargo test --workspace` green; totals match the pre-batch baseline
  (683 runner lib + 16 effigy-managed + 89 effigy-env = 788, modulo any
  test redistribution this batch incidentally triggers).
- `cargo fmt --all -- --check` clean.
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err
  -A clippy::too_many_arguments -A clippy::type_complexity` clean
  (pre-existing `effigy-doctor` and `effigy-release` warnings excepted).
- `cargo run --bin effigy -- qa:docs` pass.
- `git diff --check` clean.

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err
  -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Return to planning. The lane is between batches; the next move is either
the decide card for routing core
([`243-decide-task-routing-core-extraction-shape.md`](./243-decide-task-routing-core-extraction-shape.md))
or the decide card for built-in tasks
([`244-decide-builtin-tasks-extraction-shape.md`](./244-decide-builtin-tasks-extraction-shape.md)),
per the intent choice recorded in the 010 lane doc.
