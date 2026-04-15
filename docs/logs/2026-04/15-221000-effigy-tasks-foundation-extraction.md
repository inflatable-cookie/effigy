# Effigy Tasks Foundation Extraction

Date: 2026-04-15
Owner: Platform

## Summary

`118` is complete.

Effigy now has a real `effigy-tasks` crate in live use. This batch moved the
shared task-facing model and parsing contracts out of `runner` and into a
dedicated domain crate.

## What Changed

- added [`crates/effigy-tasks`](../../../crates/effigy-tasks/Cargo.toml)
- moved the first shared task-domain contracts there:
  - `TaskContext`
  - `TaskError`
  - `TaskSelector`
  - `TaskRuntimeArgs`
  - `CatalogSelectionMode`
- moved task selector and runtime-argument parsing into `effigy-tasks`
- reconnected the main crate so existing runtime paths now consume those types
  through the extracted crate
- kept `runner` manifest-backed catalog loading and selection execution in
  place because that code still depends directly on manifest ownership

## Why The Next Batch Is Manifest Core

This extraction exposed the next real knot:

- task-domain shared types are no longer the problem
- manifest loading, composition, and task-manifest ownership still live inside
  `runner`

So the honest next step is not another shallow task move. It is manifest-core
extraction.

## Current State

- active strict lane: `g02.010`
- active ready card: `119`
- queued release card: `115`

## Validation

- `cargo fmt --all`
- `cargo test -p effigy-tasks`
- `cargo test resolver_tests --lib`
- `cargo test run_manifest_task_builtin_argument_contract_matrix_is_stable --lib`

## Vision Target Delta

- primary vision tags touched: `MAINT`, `CONTRACT`, `RELEASE`
- moved from `task-domain model and parsing owned by the main crate` to
  `task-domain model and parsing owned by a dedicated workspace crate`
- remains open:
  - manifest-core extraction
  - deeper task/catalog extraction
  - later release/distribution/container/demo extraction
  - eventual resume of `g02.007` release closure for `v0.3`

## Next Task

Execute
[`119-implement-manifest-core-foundation-extraction.md`](../../specs/batch-cards/119-implement-manifest-core-foundation-extraction.md)
to move shared manifest contracts out of `runner`.
