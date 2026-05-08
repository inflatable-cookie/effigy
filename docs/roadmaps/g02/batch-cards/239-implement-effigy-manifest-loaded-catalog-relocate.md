# 239 Implement Effigy Manifest LoadedCatalog Relocate

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Relocate `LoadedCatalog`, `TaskSelection`, and `DeferredCommand` from
`src/runner/model/catalog.rs` into `effigy-manifest` so the upcoming
managed extraction (`240`) doesn't have to carry the 78 cross-runner
call sites along with it.

This card implements decision `3` from `238`.

## Context

- `LoadedCatalog` owns a `TaskManifest` from `effigy-manifest`; it is a
  manifest-loading concept, not a runtime-managed concept.
- 78 call sites across `src/runner/**` already treat it as a first-class
  concrete type (grep anchor: `LoadedCatalog`).
- `TaskSelection` and `DeferredCommand` sit in the same file and share
  the concern; they travel with.
- `runner::model::catalog` also re-exports `effigy_tasks::{TaskSelector,
  TaskRuntimeArgs, CatalogSelectionMode}`; those stay as re-exports.

## In Scope

- Add `LoadedCatalog`, `TaskSelection`, `DeferredCommand` to
  `effigy-manifest` (public), with public fields at the same shape they
  have today.
- `effigy-manifest` already has `effigy-tasks` / `effigy-core` reach;
  `TaskSelection` will need whichever of those types it references to
  be imported from their real homes.
- Rewrite all 78 `src/runner/**` call sites to
  `use effigy_manifest::{LoadedCatalog, ...}` (mechanical).
- Leave `src/runner/model/catalog.rs` as a thin re-export shim:
  `pub(in crate::runner) use effigy_manifest::{...};` plus the existing
  re-exports of `effigy_tasks::{...}`.
- Update `runner::error::RunnerError` adapter paths if any mention the
  old type path explicitly.

## Out Of Scope

- Moving any `src/runner/managed/**` code (that's `240`).
- Rewriting external consumers of `runner::managed` (follow-up sweep).
- Relocating `ManifestTask`, `TaskManifest`, or any other manifest types
  (already in `effigy-manifest`).

## Acceptance Criteria

- `effigy-manifest` exports `LoadedCatalog`, `TaskSelection`,
  `DeferredCommand` as public types.
- `src/runner/model/catalog.rs` is a thin shim (no type definitions).
- All 78 prior call sites compile against the new path (either via
  direct `effigy_manifest::` imports or via the shim).
- `cargo test --workspace` green.
- `cargo fmt --all -- --check` and `cargo clippy --all-targets -- -D
  warnings -A clippy::result_large_err -A clippy::too_many_arguments -A
  clippy::type_complexity` clean.

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
to extract `src/runner/managed/**` and the two
`runner::model::managed` shapes into a new `effigy-managed` crate now
that `LoadedCatalog` lives in a neutral home.
