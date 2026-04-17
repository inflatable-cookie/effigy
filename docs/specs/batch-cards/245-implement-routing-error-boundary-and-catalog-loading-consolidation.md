# 245 Implement Routing Error Boundary and Catalog-Loading Consolidation

Status: ready
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Prerequisite for the `effigy-routing` extraction (`246`). Do the
boundary and glue work entirely inside the runner so `246` itself can be
a near-mechanical crate move. Two pieces:

1. Introduce a `RoutingError` enum and route the seven catalog-owned
   `RunnerError` variants through it with `From<RoutingError> for
   RunnerError`.
2. Consolidate the `load_task_manifest` + `TASK_MANIFEST_FILE`
   catalog-loading glue that currently lives in `runner/manifest.rs` and
   `runner/model/constants` into a shape the future `effigy-routing`
   crate can own directly, without dragging scan's parallel usage along.

## Context

Card `243` decided the routing extraction shape. The coupling sweep
surfaced two pre-extraction cleanups:

- The seven variants `TaskCatalogsMissing`, `TaskCatalogReadDir`,
  `TaskCatalogAliasConflict`, `TaskCatalogPrefixNotFound`, `TaskNotFound`,
  `TaskNotFoundAny`, `TaskAmbiguous` are produced only by
  `src/runner/catalog/**` and `src/runner/manifest.rs` helpers that
  catalog calls into. They are a natural `RoutingError` subset, matching
  the Job-8 pattern used by `effigy-process::ProcessManagerError`,
  `effigy-ui::UiError`, `effigy-managed::ManagedError`, and
  `effigy-env::EnvSchemaError`.
- `deferral::policy.rs` pattern-matches `TaskNotFoundAny`,
  `TaskCatalogPrefixNotFound`, `TaskNotFound`. The `From<RoutingError>
  for RunnerError` impl must reproduce the same variant shapes so
  deferral's matcher keeps working without any deferral changes.
- `src/runner/catalog/discovery.rs` and
  `src/runner/scan/options/loading/common.rs` both reach into
  `src/runner/manifest.rs::load_task_manifest` and
  `src/runner/model/constants::TASK_MANIFEST_FILE`. If catalog extracts
  without first consolidating its side, the extraction drags both
  helpers into `effigy-routing` even though scan still depends on them
  in the runner — a 238-lesson hazard.

## In Scope

### Part A — RoutingError introduction

- Define `RoutingError` inside the runner (temporary home;
  `src/runner/catalog/error.rs` or similar). Variants match the seven
  identified:
  - `TaskCatalogsMissing { root: PathBuf }`
  - `TaskCatalogReadDir { path: PathBuf, error: std::io::Error }`
  - `TaskCatalogAliasConflict { alias: String, first_path: PathBuf, second_path: PathBuf }`
  - `TaskCatalogPrefixNotFound { prefix: String, available: Vec<String> }`
  - `TaskNotFound { name: String, path: PathBuf }`
  - `TaskNotFoundAny { name: String, catalogs: Vec<String> }`
  - `TaskAmbiguous { name: String, candidates: Vec<String> }`
- Display/Error trait impls move with the variants (lift from
  `src/runner/error/display.rs`).
- `impl From<RoutingError> for RunnerError` reproduces the same
  `RunnerError::Task*` variant shapes. Keep the `RunnerError` variants
  in place (they remain the runner's public error face).
- Update every producer inside `catalog/**` (plus any helpers in
  `runner/manifest.rs` that produce these variants) to return
  `RoutingError` at its narrow boundary; adapters at the call sites
  lift to `RunnerError` via `?`.

### Part B — Catalog-loading glue consolidation

- Identify the minimal catalog-side use of `load_task_manifest` and
  `TASK_MANIFEST_FILE`. Relocate that usage into a catalog-owned module
  (e.g. `src/runner/catalog/manifest_load.rs`) that catalog calls
  directly, without routing through `runner/manifest.rs`'s shim.
- Scan's parallel usage in
  `src/runner/scan/options/loading/common.rs` stays pointed at
  `runner/manifest.rs`. Do NOT migrate scan in this card — scan will be
  addressed by its own future decide card.
- `TASK_MANIFEST_FILE` itself can either travel to the catalog module or
  stay in `runner/model/constants` until the `246` move. Prefer
  travelling if that reduces the number of files `246` touches.

## Out Of Scope

- Moving any code into a new crate — that is `246`.
- Touching scan, locking, deferral, or `runner/manifest.rs` beyond what
  catalog-side consolidation requires.
- Renaming `RunnerError::Task*` variants — they keep their current
  shapes for compatibility with deferral's pattern-match and with
  `RunnerError`'s rendered-output expectations.
- Relocating `task_lock_scope` (sweep-flagged but belongs to the future
  locking extraction).

## Acceptance Criteria

- `RoutingError` enum exists inside the runner with the seven variants.
- `impl From<RoutingError> for RunnerError` is in place; call sites use
  `?` to lift.
- `src/runner/catalog/**` functions return `Result<_, RoutingError>` at
  their narrow boundary; RunnerError appears only at adapter layers.
- Display / Error trait impls move with the variants; error output is
  byte-identical for the seven variants (validate via existing error
  tests).
- `src/runner/catalog/**` no longer reaches into
  `src/runner/manifest.rs` for catalog-specific loading; it owns its
  own `load_task_manifest` call path.
- Scan continues to compile and pass tests unchanged.
- Test totals unchanged: 683 runner lib + 16 effigy-managed + 89
  effigy-env.

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

Open card [`246`](./246-implement-effigy-routing-extraction.md) —
implement the `effigy-routing` crate extraction. `246` is currently
queued behind this card.
