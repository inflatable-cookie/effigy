# 246 Implement effigy-routing Extraction

Status: archived
Updated: 2026-04-17
Roadmap: `g02.010`
Spec: `docs/specs/010-effigy-modularization-and-crate-boundaries-strict-lane.md`

## Objective

Extract `src/runner/catalog/**` plus the consolidated catalog-loading
glue and `RoutingError` enum (from `245`) into a new `effigy-routing`
workspace crate. Follow the established extraction pattern
(`effigy-process`, `effigy-ui`, `effigy-managed`): new crate, narrow
error boundary, `From` impl at the runner's edge, call sites migrate to
the new crate's import path.

## Context

Card `243` decided the routing extraction shape. Card `245` is the
prerequisite — it does the error-boundary introduction and
catalog-loading glue consolidation entirely inside the runner. With
`245` landed, `246` becomes a near-mechanical crate move.

Scope (~500 lines plus consolidated glue):

- `src/runner/catalog/**` — discovery, selection, module root
- `runner/catalog/manifest_load.rs` — added by `245`
- `RoutingError` enum + Display/Error impls — added by `245`

Inbound callers (from the `243` sweep):

- `runner/demo_command.rs`, `runner/execute/preflight/context/discovery.rs`
- `runner/execute/selection.rs`,
  `runner/execute/pipeline/{managed,command}.rs`
- `runner/builtin/mod.rs`,
  `runner/builtin/completion/candidates/cache/manifests.rs`,
  `runner/builtin/test/planning/resolve/target_config.rs`
- `runner/tasks_command/prepare.rs`, `runner/tasks_probe/resolve.rs`
- `runner/doctor/references.rs`, `runner/doctor/explain.rs`
- `runner/test_support.rs`

`runner/manifest.rs` continues to exist for scan and any other
non-catalog consumers. `effigy-manifest` already owns `LoadedCatalog`,
`TaskSelection`, `TaskResolverFn`, so the new crate leans on that data
layer rather than duplicating it.

## In Scope

- Create `crates/effigy-routing/` workspace crate with `Cargo.toml`,
  `src/lib.rs`.
- Move `src/runner/catalog/**` contents into `crates/effigy-routing/src/`.
- Move `RoutingError` into the new crate.
- Move the consolidated catalog-loading glue into the new crate.
- Add deps: `effigy-core` (if needed), `effigy-manifest`, `effigy-tasks`,
  `std`/`serde`/`toml` as required.
- Remove `src/runner/catalog/` directory from the runner.
- Update `src/runner/error.rs`: change `impl From<RoutingError> for
  RunnerError` from the runner-internal variant to depend on
  `effigy_routing::RoutingError`. `RunnerError::Task*` variants stay —
  the From impl continues to lift them.
- Migrate the ~12 caller files to import from `effigy_routing::` instead
  of `crate::runner::catalog::`.
- Update `Cargo.toml` workspace `members` list.
- Add `effigy-routing = { path = "crates/effigy-routing" }` to the root
  crate's deps.

## Out Of Scope

- Moving scan, locking, or deferral (each gets its own future decide
  card).
- Relocating `task_lock_scope` (belongs to the locking extraction).
- Any builtin-tasks work (that is card `244`).
- Changes to `RunnerError`'s variant shapes — deferral's pattern-match
  must keep working.

## Acceptance Criteria

- `effigy-routing` workspace crate exists with the moved code.
- `src/runner/catalog/` directory is gone.
- `impl From<RoutingError> for RunnerError` lives in `src/runner/error.rs`
  and depends on `effigy_routing::RoutingError`.
- All ~12 caller files import from `effigy_routing::` directly (no
  transitional shim inside `src/`).
- `src/lib.rs` does not gain a new `pub use effigy_routing::*` re-export
  — consumers import from the crate directly (242/second-sweep lesson).
- `effigy-managed` continues to use `effigy_manifest::TaskResolverFn`
  unchanged (the callback contract is unaffected).
- Scan's `runner/manifest.rs` usage still works — no scan changes
  required.
- Deferral's pattern-match on `RunnerError::TaskNotFound*` /
  `TaskCatalogPrefixNotFound` still compiles and works.
- Test totals unchanged: 683 runner lib + 16 effigy-managed + 89
  effigy-env + any new effigy-routing unit tests (flag in
  post-extraction checkpoint).

## Validation

- `cargo test --workspace`
- `cargo fmt --all -- --check`
- `cargo clippy --all-targets -- -D warnings -A clippy::result_large_err -A clippy::too_many_arguments -A clippy::type_complexity`
- `cargo run --bin effigy -- qa:docs`
- `git diff --check`

## Next Task

_To be decided after this card lands — candidate follow-ups: decide
card for scan extraction (~4.9k lines), decide card for locking
extraction (~410 lines), or pivot to card `244` (builtin tasks
extraction shape)._
